use anyhow::anyhow;
use core::fmt;
use langchain_rust::vectorstore::pgvector::Store;
use langchain_rust::vectorstore::VectorStore;
use langchain_rust::{schemas::Document, vectorstore::VecStoreOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

pub const COLLECTION_NAME: &str = "advisor";

use crate::edgar::report::ReportType;
use crate::ProgressTracker;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocType {
    EdgarFiling,
    EarningTranscript,
}

impl fmt::Display for DocType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DocType::EdgarFiling => write!(f, "EdgarFiling"),
            DocType::EarningTranscript => write!(f, "EarningTranscript"),
        }
    }
}

impl FromStr for DocType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        log::info!("!: {}", s);
        match s {
            "edgar_filing" => Ok(DocType::EdgarFiling),
            "earnings_transcript" => Ok(DocType::EarningTranscript),
            _ => Err(anyhow!("Unknown document type: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Metadata {
    MetaEarningsTranscript {
        doc_type: DocType,
        filepath: PathBuf,
        symbol: String,
        year: usize,
        quarter: usize,
        chunk_index: usize,
        total_chunks: usize,
    },
    MetaEdgarFiling {
        doc_type: DocType,
        filepath: PathBuf,
        symbol: String,
        filing_type: ReportType,
        cik: String,
        accession_number: String,
        chunk_index: usize,
        total_chunks: usize,
    },
}

impl Metadata {
    fn set_chunks(&mut self, index: usize, total: usize) -> &mut Self {
        match self {
            Metadata::MetaEarningsTranscript {
                chunk_index,
                total_chunks,
                ..
            } => {
                *chunk_index = index;
                *total_chunks = total;
            }
            Metadata::MetaEdgarFiling {
                chunk_index,
                total_chunks,
                ..
            } => {
                *chunk_index = index;
                *total_chunks = total;
            }
        }
        self
    }

    pub fn symbol(&self) -> &String {
        match self {
            Metadata::MetaEarningsTranscript { symbol, .. } => symbol,
            Metadata::MetaEdgarFiling { symbol, .. } => symbol,
        }
    }

    pub fn filepath(&self) -> &PathBuf {
        match self {
            Metadata::MetaEarningsTranscript { filepath, .. } => filepath,
            Metadata::MetaEdgarFiling { filepath, .. } => filepath,
        }
    }

    pub fn doc_type(&self) -> DocType {
        match self {
            Metadata::MetaEarningsTranscript { doc_type, .. } => doc_type.clone(),
            Metadata::MetaEdgarFiling { doc_type, .. } => doc_type.clone(),
        }
    }

    pub fn chunk_index(&self) -> usize {
        match self {
            Metadata::MetaEarningsTranscript { chunk_index, .. } => *chunk_index,
            Metadata::MetaEdgarFiling { chunk_index, .. } => *chunk_index,
        }
    }

    pub fn total_chunks(&self) -> usize {
        match self {
            Metadata::MetaEarningsTranscript { total_chunks, .. } => *total_chunks,
            Metadata::MetaEdgarFiling { total_chunks, .. } => *total_chunks,
        }
    }
}

impl From<Metadata> for HashMap<String, Value> {
    fn from(val: Metadata) -> Self {
        let mut map = HashMap::new();
        match val {
            Metadata::MetaEdgarFiling {
                filepath,
                filing_type,
                cik,
                accession_number,
                symbol,
                chunk_index,
                total_chunks,
                doc_type,
            } => {
                map.insert("doc_type".to_string(), Value::String(doc_type.to_string()));
                map.insert(
                    "filepath".to_string(),
                    Value::String(filepath.to_str().unwrap_or("unknown").to_string()),
                );
                map.insert(
                    "filing_type".to_string(),
                    Value::String(filing_type.to_string()),
                );
                map.insert("cik".to_string(), Value::String(cik));
                map.insert(
                    "accession_number".to_string(),
                    Value::String(accession_number),
                );
                map.insert("symbol".to_string(), Value::String(symbol));
                map.insert(
                    "chunk_index".to_string(),
                    Value::Number(serde_json::Number::from(chunk_index)),
                );
                map.insert(
                    "total_chunks".to_string(),
                    Value::Number(serde_json::Number::from(total_chunks)),
                );
            }
            Metadata::MetaEarningsTranscript {
                filepath,
                symbol,
                quarter,
                year,
                chunk_index,
                total_chunks,
                doc_type,
            } => {
                map.insert("doc_type".to_string(), Value::String(doc_type.to_string()));
                map.insert(
                    "filepath".to_string(),
                    Value::String(filepath.to_str().unwrap_or("unknown").to_string()),
                );
                map.insert("symbol".to_string(), Value::String(symbol));
                map.insert(
                    "quarter".to_string(),
                    Value::Number(serde_json::Number::from(quarter)),
                );
                map.insert(
                    "year".to_string(),
                    Value::Number(serde_json::Number::from(year)),
                );
                map.insert(
                    "chunk_index".to_string(),
                    Value::Number(serde_json::Number::from(chunk_index)),
                );
                map.insert(
                    "total_chunks".to_string(),
                    Value::Number(serde_json::Number::from(total_chunks)),
                );
            }
        }
        map
    }
}

const CHUNK_SIZE: usize = 4000; // Characters per chunk, keeping well under token limits

pub async fn store_chunked_document(
    content: String,
    metadata: Metadata,
    store: Arc<Store>,
    pg_pool: &Pool<Postgres>,
    progress_tracker: Option<&ProgressTracker>,
) -> anyhow::Result<()> {
    log::debug!("Storing document with metadata: {:?}", metadata);
    if let Some(tracker) = progress_tracker {
        tracker.start_progress(100, "Downloading [Document storage]");
    }

    // Split content into smaller chunks
    let chunks: Vec<String> = content
        .chars()
        .collect::<Vec<char>>()
        .chunks(CHUNK_SIZE)
        .map(|c| c.iter().collect::<String>())
        .collect();

    log::info!("Checking if document already exists in the database");

    let fp = metadata.filepath().to_str().unwrap();
    let count = crate::db::count_vectors_with_filepath(pg_pool, fp).await?;

    if count > 0 {
        log::info!(
            "Document already exists in the database (found {} matches), skipping embedding",
            count
        );
        return Ok(());
    }

    // Collect all document chunks
    let mut documents = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let mut chunk_metadata = metadata.clone();
        chunk_metadata.set_chunks(i, chunks.len());

        let doc = Document {
            page_content: chunk.clone(),
            metadata: chunk_metadata.into(),
            score: 0.0,
        };

        documents.push(doc);
    }

    log::info!(
        "Attempting to store {} documents in vector store.",
        documents.len(),
    );

    match store
        .add_documents(&documents, &VecStoreOptions::default())
        .await
    {
        Ok(_) => {
            log::info!(
                "Successfully added {} documents to vector store with types: {:?}",
                documents.len(),
                documents
                    .iter()
                    .map(|d| d
                        .metadata
                        .get("filing_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"))
                    .collect::<HashSet<_>>()
            );
            Ok(())
        }
        Err(e) => {
            log::error!(
                "Failed to store documents in vector store: {}\nAttempted to store documents with metadata: {:#?}",
                e,
                documents.iter().map(|d| &d.metadata).collect::<Vec<_>>()
            );
            Err(anyhow!(
                "Failed to store document chunks in vector store: {}",
                e
            ))
        }
    }?;

    log::info!("Stored {} document chunks in vector store", documents.len());
    if let Some(tracker) = progress_tracker {
        tracker.finish(); // Keep progress bar visible
    }
    Ok(())
}

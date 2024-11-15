use anyhow::anyhow;
use core::fmt;
use langchain_rust::vectorstore::VectorStore;
use langchain_rust::{schemas::Document, vectorstore::VecStoreOptions};
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::edgar::report::ReportType;

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

    fn symbol(&self) -> &String {
        match self {
            Metadata::MetaEarningsTranscript { symbol, .. } => symbol,
            Metadata::MetaEdgarFiling { symbol, .. } => symbol,
        }
    }

    fn filepath(&self) -> &PathBuf {
        match self {
            Metadata::MetaEarningsTranscript { filepath, .. } => filepath,
            Metadata::MetaEdgarFiling { filepath, .. } => filepath,
        }
    }

    fn doc_type(&self) -> DocType {
        match self {
            Metadata::MetaEarningsTranscript { doc_type, .. } => doc_type.clone(),
            Metadata::MetaEdgarFiling { doc_type, .. } => doc_type.clone(),
        }
    }

    fn chunk_index(&self) -> usize {
        match self {
            Metadata::MetaEarningsTranscript { chunk_index, .. } => *chunk_index,
            Metadata::MetaEdgarFiling { chunk_index, .. } => *chunk_index,
        }
    }

    fn total_chunks(&self) -> usize {
        match self {
            Metadata::MetaEarningsTranscript { total_chunks, .. } => *total_chunks,
            Metadata::MetaEdgarFiling { total_chunks, .. } => *total_chunks,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataJson {
    pub doc_type: String,
    pub filepath: String,
    pub filing_type: Option<ReportType>,
    pub cik: Option<String>,
    pub accession_number: Option<String>,
    pub symbol: String,
    pub quarter: Option<usize>,
    pub year: Option<usize>,
    pub chunk_index: usize,
    pub total_chunks: usize,
}

impl Into<HashMap<String, Value>> for Metadata {
    fn into(self: Metadata) -> HashMap<String, Value> {
        match self {
            Metadata::MetaEdgarFiling {
                filepath,
                filing_type,
                cik,
                accession_number,
                symbol,
                chunk_index,
                total_chunks,
                ..
            } => hashmap! {
                "doc_type".to_string() => Value::String("filing".to_string()),
                "filepath".to_string() => Value::String(filepath.to_str().unwrap_or("unknown").to_string()),
                "filing_type".to_string() => Value::String(filing_type.to_string()),
                "cik".to_string() => Value::String(cik),
                "accession_number".to_string() => Value::String(accession_number),
                "symbol".to_string() => Value::String(symbol),
                "quarter".to_string() => Value::Null,
                "year".to_string() => Value::Null,
                "chunk_index".to_string() => Value::Number(chunk_index.into()),
                "total_chunks".to_string() => Value::Number(total_chunks.into())
            },
            Metadata::MetaEarningsTranscript {
                filepath,
                symbol,
                quarter,
                year,
                chunk_index,
                total_chunks,
                ..
            } => hashmap! {
                "doc_type".to_string() => Value::String("filing".to_string()),
                "filepath".to_string() => Value::String(filepath.to_str().unwrap_or("unknown").to_string()),
                "quarter".to_string() => Value::Number(quarter.into()),
                "year".to_string() => Value::Number(year.into()),
                "symbol".to_string() => Value::String(symbol),
                "quarter".to_string() => Value::Null,
                "year".to_string() => Value::Null,
                "chunk_index".to_string() => Value::Number(chunk_index.into()),
                "total_chunks".to_string() => Value::Number(total_chunks.into())
            },
        }
    }
}

const CHUNK_SIZE: usize = 4000; // Characters per chunk, keeping well under token limits

pub async fn store_chunked_document(
    content: String,
    metadata: Metadata,
    store: &dyn VectorStore,
) -> anyhow::Result<()> {
    println!("Storing document with metadata: {:?}", metadata);

    // Split content into smaller chunks
    let chunks: Vec<String> = content
        .chars()
        .collect::<Vec<char>>()
        .chunks(CHUNK_SIZE)
        .map(|c| c.iter().collect::<String>())
        .collect();

    // Check if the vector store is persistent (e.g., Qdrant, SQLite) and if documents already exist
    log::info!("Checking if document already exists in persistent vector store");

    // Create a proper filter to check for existing documents
    let filter = match metadata {
        Metadata::MetaEdgarFiling {
            ref filing_type,
            ref cik,
            ref accession_number,
            ..
        } => {
            serde_json::json!({
                "must": [
                    {
                        "key": "type",
                        "match": { "value": "edgar_filing" }
                    },
                    {
                        "key": "filing_type",
                        "match": { "value": filing_type.to_string() }
                    },
                    {
                        "key": "cik",
                        "match": { "value": cik }
                    },
                    {
                        "key": "accession_number",
                        "match": { "value": accession_number }
                    }
                ]
            })
        }
        Metadata::MetaEarningsTranscript {
            ref symbol,
            ref year,
            ref quarter,
            ..
        } => {
            serde_json::json!({
                "must": [
                    {
                        "key": "type",
                        "match": { "value": "earnings_transcript" }
                    },
                    {
                        "key": "symbol",
                        "match": { "value": symbol }
                    },
                    {
                        "key": "quarter",
                        "match": { "value": quarter }
                    },
                    {
                        "key": "year",
                        "match": { "value": year }
                    }
                ]
            })
        }
    };

    log::info!("FILTER: {}", filter);

    log::debug!("Checking for existing documents with filter: {}", filter);

    // Perform a similarity search to check if the document is already stored
    let existing_docs = store
        .similarity_search(&filter.to_string(), 1, &VecStoreOptions::default())
        .await
        .map_err(|e| anyhow!("Failed to check for existing documents: {}", e))?;

    if !existing_docs.is_empty() {
        log::info!(
            "Document already exists in vector store (found {} matches), skipping embedding",
            existing_docs.len()
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
        "Attempting to store {} documents in vector store with metadata:\n{:#?}",
        documents.len(),
        documents.iter().map(|d| &d.metadata).collect::<Vec<_>>()
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
    Ok(())
}

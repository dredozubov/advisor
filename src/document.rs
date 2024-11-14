use anyhow::anyhow;
use core::fmt;
use langchain_rust::vectorstore::VectorStore;
use langchain_rust::{schemas::Document, vectorstore::VecStoreOptions};
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Display;
use std::{collections::HashMap, path::Path};

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
        filepath: String,
        symbol: String,
        year: usize,
        quarter: usize,
        chunk_index: usize,
        total_chunks: usize,
    },
    MetaEdgarFiling {
        doc_type: DocType,
        filepath: String,
        filing_type: ReportType,
        cik: String,
        accession_number: String,
        symbol: String,
        chunk_index: usize,
        total_chunks: usize,
    },
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

fn to_hashmap(metadata: Metadata) -> MetadataJson {
    match metadata {
        Metadata::MetaEdgarFiling {
            doc_type,
            filepath,
            filing_type,
            cik,
            accession_number,
            symbol,
            chunk_index,
            total_chunks,
        } => MetadataJson {
            doc_type: "filing".to_string(),
            filepath,
            filing_type: Some(filing_type),
            cik: Some(cik),
            accession_number: Some(accession_number),
            symbol,
            quarter: None,
            year: None,
            chunk_index,
            total_chunks,
        },
        Metadata::MetaEarningsTranscript {
            doc_type,
            filepath,
            symbol,
            quarter,
            year,
            chunk_index,
            total_chunks,
        } => MetadataJson {
            doc_type: "earnings_transcript".to_string(),
            filepath,
            filing_type: None,
            cik: None,
            accession_number: None,
            symbol,
            quarter: Some(quarter),
            year: Some(year),
            chunk_index,
            total_chunks,
        },
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

    log::info!(
        "Created {} chunks for document type '{}'",
        chunks.len(),
        &metadata.doc_type,
    );

    // Check if the vector store is persistent (e.g., Qdrant, SQLite) and if documents already exist
    log::info!("Checking if document already exists in persistent vector store");

    // Create a proper filter to check for existing documents
    let filter = match metadata.doc_type {
        DocType::EdgarFiling => {
            let filing_type = metadata.filing_type.as_deref().unwrap_or("unknown");
            let cik = metadata.cik.as_deref().unwrap_or("unknown");
            let accession_number = metadata.accession_number.as_deref().unwrap_or("unknown");
            serde_json::json!({
                "must": [
                    {
                        "key": "type",
                        "match": { "value": "edgar_filing" }
                    },
                    {
                        "key": "filing_type",
                        "match": { "value": filing_type }
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
        DocType::EarningTranscript => {
            let symbol = metadata.symbol;
            let quarter = metadata.quarter.unwrap_or("unknown".to_string());
            let year = metadata.year;
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
        chunk_metadata.chunk_index = i;
        chunk_metadata.total_chunks = chunks.len();

        let doc = Document {
            page_content: chunk.clone(),
            metadata: to_hashmap(metadata.clone()), // Use the to_hashmap function to convert Metadata to MetadataJson
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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub doc_type: String,
    pub filepath: Option<String>,
    pub filing_type: Option<String>,
    pub cik: Option<String>,
    pub accession_number: Option<String>,
    pub symbol: Option<String>,
    pub quarter: Option<String>,
    pub year: Option<String>,
    pub chunk_index: Option<usize>,
    pub total_chunks: Option<usize>,
}

use anyhow::anyhow;
use langchain_rust::vectorstore::VectorStore;
use langchain_rust::{schemas::Document, vectorstore::VecStoreOptions};
use serde_json::Value;
use std::collections::HashSet;
use std::{collections::HashMap, path::Path};

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

    // Extract document type and identifier from metadata
    let doc_type = &metadata.doc_type;
    let identifier = match doc_type.as_str() {
        "edgar_filing" => metadata.filepath.as_deref().unwrap_or("unknown"),
        "earnings_transcript" => metadata.symbol.as_deref().unwrap_or("unknown"),
        _ => "unknown",
    };

    log::info!(
        "Created {} chunks for document type '{}' ({})",
        chunks.len(),
        doc_type,
        identifier
    );

    // Check if the vector store is persistent (e.g., Qdrant, SQLite) and if documents already exist
    log::info!("Checking if document already exists in persistent vector store");

    // Create a proper filter to check for existing documents
    let filter = match metadata.doc_type.as_str() {
        "edgar_filing" => {
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
        "earnings_transcript" => {
            let symbol = metadata
                .get("symbol")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let quarter = metadata
                .get("quarter")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let year = metadata
                .get("year")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
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
        _ => serde_json::json!({
            "must": [
                {
                    "key": "type",
                    "match": { "value": doc_type }
                }
            ]
        }),
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
        chunk_metadata.chunk_index = Some(i);
        chunk_metadata.total_chunks = Some(chunks.len());

        let doc = Document {
            page_content: chunk.clone(),
            metadata: chunk_metadata,
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

use anyhow::anyhow;
use langchain_rust::vectorstore::VectorStore;
use langchain_rust::{schemas::Document, vectorstore::VecStoreOptions};
use serde_json::Value;
use std::{collections::HashMap, path::Path};

const CHUNK_SIZE: usize = 4000; // Characters per chunk, keeping well under token limits

pub async fn store_chunked_document(
    content: String,
    metadata: HashMap<String, Value>,
    store: &dyn VectorStore,
) -> anyhow::Result<()> {
    // Split content into smaller chunks
    let chunks: Vec<String> = content
        .chars()
        .collect::<Vec<char>>()
        .chunks(CHUNK_SIZE)
        .map(|c| c.iter().collect::<String>())
        .collect();

    // Extract document type and identifier from metadata
    let doc_type = metadata
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let identifier = match doc_type {
        "edgar_filing" => metadata
            .get("filepath")
            .and_then(|v| v.as_str())
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
            })
            .unwrap_or("unknown"),
        "earnings_transcript" => metadata
            .get("symbol")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
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
    let filter = match doc_type {
        "edgar_filing" => {
            let filing_type = metadata.get("filing_type").and_then(|v| v.as_str()).unwrap_or("unknown");
            let cik = metadata.get("cik").and_then(|v| v.as_str()).unwrap_or("unknown");
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
                    }
                ]
            })
        },
        "earnings_transcript" => {
            let symbol = metadata.get("symbol").and_then(|v| v.as_str()).unwrap_or("unknown");
            let quarter = metadata.get("quarter").and_then(|v| v.as_str()).unwrap_or("unknown");
            let year = metadata.get("year").and_then(|v| v.as_str()).unwrap_or("unknown");
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
        },
        _ => serde_json::json!({
            "must": [
                {
                    "key": "type",
                    "match": { "value": doc_type }
                }
            ]
        })
    };

    log::debug!("Checking for existing documents with filter: {}", filter);
    
    // Perform a similarity search to check if the document is already stored
    let existing_docs = store
        .similarity_search(
            &filter.to_string(),
            1,
            &VecStoreOptions::default()
        )
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
        chunk_metadata.insert("chunk_index".to_string(), serde_json::json!(i));
        chunk_metadata.insert("total_chunks".to_string(), serde_json::json!(chunks.len()));

        let doc = Document {
            page_content: chunk.clone(),
            metadata: chunk_metadata,
            score: 0.0,
        };

        documents.push(doc);
    }

    // Send all chunks in a single request
    log::debug!(
        "Adding {} chunks to vector store with metadata: {:?}",
        documents.len(),
        documents.first().map(|d| &d.metadata)
    );

    if log::log_enabled!(log::Level::Debug) {
        log::debug!(
            "First chunk content: {:?}",
            documents.first().map(|d| &d.page_content)
        );
    }

    log::debug!(
        "Storing documents in vector store with metadata sample: {:#?}",
        documents.first().map(|d| &d.metadata)
    );

    log::info!(
        "Attempting to store {} documents in vector store with first document metadata: {:#?}",
        documents.len(),
        documents.first().map(|d| &d.metadata)
    );

    log::debug!(
        "First document content preview (first 200 chars): {}",
        documents.first()
            .map(|d| d.page_content.chars().take(200).collect::<String>())
            .unwrap_or_default()
    );

    match store
        .add_documents(&documents, &VecStoreOptions::default())
        .await
    {
        Ok(_) => {
            log::info!("Successfully added {} documents to vector store", documents.len());
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to store documents in vector store: {}", e);
            Err(anyhow!("Failed to store document chunks in vector store: {}", e))
        }
    }?;

    log::info!("Stored {} document chunks in vector store", documents.len());
    Ok(())
}

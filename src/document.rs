use crate::storage::VectorStorage;
use anyhow::anyhow;
use langchain_rust::{schemas::Document, vectorstore::VecStoreOptions};
use serde_json::Value;
use std::{collections::HashMap, fs, path::Path};

const CHUNK_SIZE: usize = 4000; // Characters per chunk, keeping well under token limits

/// Store a document in chunks, with caching support
pub async fn store_chunked_document_with_cache(
    content: String,
    metadata: HashMap<String, Value>,
    cache_dir: &str,
    cache_filename: &str,
    store: &dyn VectorStorage,
) -> anyhow::Result<()> {
    // Construct the cache path
    let cache_path = format!("{}/{}.json", cache_dir, cache_filename);

    // Check if cached JSON exists
    if fs::read_to_string(cache_path.as_str()).is_ok() {
        log::info!("Using cached JSON from: {}", cache_path);
        return Ok(());
    }

    // If no cache, proceed with chunking and storing
    log::info!("No cached JSON found, chunking and storing document");

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
    if !(*store).is_ephemeral() {
        log::info!("Checking if document already exists in persistent vector store");

        // Perform a similarity search to check if the document is already stored
        let existing_docs = store
            .similarity_search(&identifier, 1, &VecStoreOptions::default())
            .await
            .map_err(|e| anyhow!("Failed to check for existing documents: {}", e))?;

        if !existing_docs.is_empty() {
            log::info!("Document already exists in vector store, skipping embedding");
            return Ok(());
        }
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
    store
        .add_documents(&documents, &VecStoreOptions::default())
        .await
        .map_err(|e| anyhow!("Failed to store document chunks in vector storage: {}", e))?;

    // Cache the chunked document
    log::info!("Caching chunked document to: {}", cache_path);
    let json_content = serde_json::to_string_pretty(&metadata)?;
    fs::create_dir_all(cache_dir)?;
    fs::write(&cache_path, json_content)?;

    log::info!(
        "Stored {} document chunks in vector storage",
        documents.len()
    );
    Ok(())
}

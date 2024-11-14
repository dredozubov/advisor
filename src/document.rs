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

    // Perform a similarity search to check if the document is already stored
    let existing_docs = store
        .similarity_search(identifier, 1, &VecStoreOptions::default())
        .await
        .map_err(|e| anyhow!("Failed to check for existing documents: {}", e))?;

    if !existing_docs.is_empty() {
        log::info!("Document already exists in vector store, skipping embedding");
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

    store
        .add_documents(&documents, &VecStoreOptions::default())
        .await
        .map_err(|e| anyhow!("Failed to store document chunks in vector store: {}", e))?;

    log::debug!("Successfully added documents to vector store");

    log::info!("Stored {} document chunks in vector store", documents.len());
    Ok(())
}

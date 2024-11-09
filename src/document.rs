use langchain_rust::{schemas::Document, vectorstore::{VectorStore, VecStoreOptions}};
use anyhow::{Result, anyhow};
use std::{collections::HashMap, path::Path};
use serde_json::Value;

const CHUNK_SIZE: usize = 4000;  // Characters per chunk, keeping well under token limits

pub async fn store_chunked_document(
    content: String,
    metadata: HashMap<String, Value>,
    store: &dyn VectorStore,
) -> Result<()> {
    log::info!("Chunking document of {} characters", content.len());
    
    // Split content into smaller chunks
    let chunks: Vec<String> = content
        .chars()
        .collect::<Vec<char>>()
        .chunks(CHUNK_SIZE)
        .map(|c| c.iter().collect::<String>())
        .collect();
    
    // Extract document type and identifier from metadata
    let doc_type = metadata.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
    let identifier = match doc_type {
        "edgar_filing" => metadata.get("filepath")
            .and_then(|v| v.as_str())
            .map(|p| Path::new(p).file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown"))
            .unwrap_or("unknown"),
        "earnings_transcript" => metadata.get("symbol")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
        _ => "unknown"
    };
    
    log::info!(
        "Created {} chunks for document type '{}' ({})", 
        chunks.len(),
        doc_type,
        identifier
    );

    // Create documents for each chunk
    for (i, chunk) in chunks.iter().enumerate() {
        let mut chunk_metadata = metadata.clone();
        chunk_metadata.insert("chunk_index".to_string(), serde_json::json!(i));
        chunk_metadata.insert("total_chunks".to_string(), serde_json::json!(chunks.len()));
        
        let doc = Document {
            page_content: chunk.clone(),
            metadata: chunk_metadata,
            score: 0.0,
        };

        store
            .add_documents(
                &[doc],
                &VecStoreOptions::default(),
            )
            .await
            .map_err(|e| anyhow!("Failed to store document chunk in vector storage: {}", e))?;
    }

    Ok(())
}

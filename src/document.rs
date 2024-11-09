use anyhow::{Result, anyhow};


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

    // Store all chunks in a single batch query
    store
        .add_documents(
            &documents,
            &VecStoreOptions::default(),
        )
        .await
        .map_err(|e| anyhow!("Failed to store document chunks in vector storage: {}", e))?;

    Ok(())
}
use anyhow::{Result, anyhow};
use langchain_rust::{schemas::Document, vectorstore::{VectorStore, VecStoreOptions}};
use serde_json::Value;

const CHUNK_SIZE: usize = 4000;  // Characters per chunk, keeping well under token limits

/// Store a document in chunks, with caching support
pub async fn store_chunked_document_with_cache(
    content: String,
    metadata: HashMap<String, Value>,
    cache_dir: &str,
    cache_filename: &str,
    store: &dyn VectorStore,
) -> Result<()> {
    // Construct the cache path
    let cache_path = format!("{}/{}.json", cache_dir, cache_filename);
    
    // Check if cached JSON exists
    if let Ok(cached_content) = fs::read_to_string(&cache_path) {
        log::info!("Using cached JSON from: {}", cache_path);
        let cached_facts: HashMap<String, Value> = serde_json::from_str(&cached_content)?;
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

    // Cache the chunked document
    log::info!("Caching chunked document to: {}", cache_path);
    let json_content = serde_json::to_string_pretty(&metadata)?;
    fs::create_dir_all(cache_dir)?;
    fs::write(&cache_path, json_content)?;

    Ok(())
}

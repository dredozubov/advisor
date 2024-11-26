use anyhow::Result;
use langchain_rust::schemas::Document;
use langchain_rust::vectorstore::VectorStore;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

const CHUNK_SIZE: usize = 4000; // Characters per chunk

use langchain_rust::embedding::openai::OpenAiEmbedder;
use langchain_rust::llm::OpenAIConfig;
use langchain_rust::vectorstore::pgvector::StoreBuilder;
use std::env;

pub async fn get_store() -> Result<Arc<dyn VectorStore>> {
    let openai_key = env::var("OPENAI_KEY").map_err(|_| anyhow::anyhow!("OPENAI_KEY not set"))?;
    let database_url =
        env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;

    let embedder =
        OpenAiEmbedder::default().with_config(OpenAIConfig::default().with_api_key(openai_key));

    let store = StoreBuilder::new()
        .embedder(embedder)
        .connection_url(&database_url)
        .collection_table_name(crate::db::COLLECTIONS_TABLE)
        .embedder_table_name(crate::db::EMBEDDER_TABLE)
        .vector_dimensions(1536)
        .build()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to build vector store: {}", e))?;

    Ok(Arc::new(store))
}

pub async fn store_document(
    content: String,
    metadata: HashMap<String, Value>,
    store: &dyn VectorStore,
) -> Result<()> {
    let chunks = chunk_document(content);
    let documents = create_documents(chunks, metadata);

    store_documents(documents, store).await
}

fn chunk_document(content: String) -> Vec<String> {
    content
        .chars()
        .collect::<Vec<char>>()
        .chunks(CHUNK_SIZE)
        .map(|c| c.iter().collect::<String>())
        .collect()
}

fn create_documents(chunks: Vec<String>, metadata: HashMap<String, Value>) -> Vec<Document> {
    chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut chunk_metadata = metadata.clone();
            chunk_metadata.insert("chunk_index".to_string(), serde_json::json!(i));
            chunk_metadata.insert("total_chunks".to_string(), serde_json::json!(chunks.len()));

            Document {
                page_content: chunk.clone(),
                metadata: chunk_metadata,
                score: 0.0,
            }
        })
        .collect()
}

async fn store_documents(documents: Vec<Document>, store: &dyn VectorStore) -> Result<()> {
    store
        .add_documents(
            &documents,
            &langchain_rust::vectorstore::VecStoreOptions::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to store documents: {}", e))?;

    Ok(())
}

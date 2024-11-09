use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{embedding::Embedder, schemas::Document, vectorstore::VectorStore};
use qdrant_client::{config::QdrantConfig, Qdrant};

use super::vector_storage::VectorStorage;

#[derive(Debug, Clone)]
pub struct QdrantStoreConfig {
    pub uri: String,
    pub collection_name: String,
}

pub struct QdrantStorage {
    client: Qdrant,
    embedder: Arc<dyn Embedder>,
}

#[async_trait]
impl VectorStore for QdrantStorage {
    async fn add_documents(&self, _documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        Ok(())
    }

    async fn similarity_search(&self, _query: &str, _limit: usize) -> Result<Vec<(Document, f32)>> {
        // TODO: Implement similarity search for Qdrant
        log::warn!("Similarity search not yet implemented for Qdrant storage");
        Ok(Vec::new())
    }

    async fn delete_documents(&self, _filter: MetadataFilter) -> Result<u64> {
        // TODO: Implement deletion with filters
        Ok(0)
    }

    async fn count(&self) -> Result<u64> {
        // TODO: Implement document counting
        Ok(0)
    }
}

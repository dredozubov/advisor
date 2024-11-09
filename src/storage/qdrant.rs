use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{embedding::Embedder, schemas::Document};
use qdrant_client::{config::QdrantConfig, Qdrant};

use super::{DocumentMetadata, EmbedderWrapper, MetadataFilter, VectorStorage};

#[derive(Debug, Clone)]
pub struct QdrantStoreConfig {
    pub uri: String,
    pub collection_name: String,
}

pub struct QdrantStorage {
    client: Qdrant,
    embedder: EmbedderWrapper,
}

#[async_trait]
impl VectorStorage for QdrantStorage {
    type Config = QdrantStoreConfig;

    async fn new(config: Self::Config, embedder: EmbedderWrapper) -> Result<Self> {
        let qdrant_config = QdrantConfig::from_url(&config.uri);
        let client = Qdrant::new(qdrant_config)
            .map_err(|e| anyhow::anyhow!("Failed to create Qdrant client: {}", e))?;

        Ok(Self { client, embedder })
    }

    fn is_in_memory(&self) -> bool {
        false
    }

    async fn add_documents(&self, _documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        // TODO: Implement document addition for Qdrant
        log::warn!("Document addition not yet implemented for Qdrant storage");
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

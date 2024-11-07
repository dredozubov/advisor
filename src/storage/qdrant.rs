use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::schemas::Document;
use qdrant_client::config::QdrantConfig;
use qdrant_client::Qdrant;

use super::{DocumentMetadata, MetadataFilter, VectorStorage};

#[derive(Debug, Clone)]
pub struct QdrantStoreConfig {
    pub uri: String,
    pub collection_name: String,
}

pub struct QdrantStorage {
    client: Qdrant,
    embedder: Arc<OpenAiEmbedder<OpenAIConfig>>,
}

#[async_trait]
impl VectorStorage for QdrantStorage {
    type Config = QdrantStoreConfig;

    async fn new(config: Self::Config, embedder: Arc<OpenAiEmbedder<OpenAIConfig>>) -> Result<Self> {
        let client = Qdrant::new(&config.uri)
            .map_err(|e| anyhow::anyhow!("Failed to create Qdrant client: {}", e))?;

        Ok(Self { client, embedder })
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

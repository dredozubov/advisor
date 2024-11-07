use std::sync::Arc;
use crate::storage::{DocumentMetadata, MetadataFilter, VectorStorage};
use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{
    embedding::openai::{OpenAiEmbedder, OpenAIConfig},
    schemas::Document,
    vectorstore::{sqlite_vss::Store as SqliteVectorStore, VectorStore, VecStoreOptions},
};

#[derive(Debug, Clone)]
pub struct SqliteConfig {
    pub path: String,
}

pub struct SqliteStorage {
    store: SqliteVectorStore,
    embedder: Arc<OpenAiEmbedder<OpenAIConfig>>,
}

#[async_trait]
impl VectorStorage for SqliteStorage {
    type Config = SqliteConfig;

    async fn new(config: Self::Config) -> Result<Self> {
        let client = QdrantClient::new(config.endpoint).await?;
        Ok(Self {
            client,
        })
    }

    async fn add_documents(&self, documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        // TODO: Implement document addition for Qdrant
        log::warn!("Document addition not yet implemented for Qdrant storage");
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize) -> Result<Vec<(Document, f32)>> {
        // TODO: Implement similarity search for Qdrant
        log::warn!("Similarity search not yet implemented for Qdrant storage");
        let results = Vec::new();
        Ok(results.into_iter().map(|doc| (doc, 1.0)).collect())
    }

    async fn delete_documents(&self, _filter: MetadataFilter) -> Result<u64> {
        // Note: The SQLiteVectorStore doesn't currently support filtered deletion
        // We could implement this later if needed
        Ok(0)
    }

    async fn count(&self) -> Result<u64> {
        // Use similarity search with empty query to get all documents
        let all_docs = self.store.similarity_search("", 1000000, &VecStoreOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to count documents: {}", e))?;
        Ok(all_docs.len() as u64)
    }
}

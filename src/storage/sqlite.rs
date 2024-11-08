use crate::storage::{DocumentMetadata, MetadataFilter, VectorStorage};
use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{
    embedding::Embedder,
    schemas::Document,
    vectorstore::{
        sqlite_vss::{Store as SqliteVectorStore, StoreBuilder},
        VecStoreOptions, VectorStore,
    },
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SqliteConfig {
    pub path: String,
}

pub struct SqliteStorage<E> {
    store: SqliteVectorStore,
    embedder: Arc<E>,
}

#[async_trait]
impl<E: Embedder + Send + Clone + Sync + 'static> VectorStorage for SqliteStorage<E> {
    type Config = SqliteConfig;
    type Embedder = E;

    async fn new(config: Self::Config, embedder: Arc<Self::Embedder>) -> Result<Self> {
        let store = StoreBuilder::new()
            .embedder(Arc::clone(&embedder))
            .connection_url(&config.path)
            .table("documents")
            .vector_dimensions(1536)
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create SQLite store: {}", e))?;

        Ok(Self { store, embedder })
    }

    async fn add_documents(&self, _documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        // TODO: Implement document addition for SQLite
        log::warn!("Document addition not yet implemented for SQLite storage");
        Ok(())
    }

    async fn similarity_search(&self, _query: &str, _limit: usize) -> Result<Vec<(Document, f32)>> {
        // TODO: Implement similarity search for SQLite
        log::warn!("Similarity search not yet implemented for SQLite storage");
        Ok(Vec::new())
    }

    async fn delete_documents(&self, _filter: MetadataFilter) -> Result<u64> {
        // Note: The SQLiteVectorStore doesn't currently support filtered deletion
        // We could implement this later if needed
        Ok(0)
    }

    async fn count(&self) -> Result<u64> {
        // Use similarity search with empty query to get all documents
        let all_docs = self
            .store
            .similarity_search("", 1000000, &VecStoreOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to count documents: {}", e))?;
        Ok(all_docs.len() as u64)
    }
}

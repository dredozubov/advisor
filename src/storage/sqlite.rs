use std::sync::Arc;
use crate::storage::{DocumentMetadata, MetadataFilter, VectorStorage};
use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{
    embedding::openai::OpenAiEmbedder,
    schemas::Document,
    vectorstore::{sqlite_vss::Store as SqliteVectorStore, VecStoreOptions},
};

#[derive(Debug, Clone)]
pub struct SqliteConfig {
    pub path: String,
}

pub struct SqliteStorage {
    store: SqliteVectorStore,
    embedder: Arc<OpenAiEmbedder>,
}

#[async_trait]
impl VectorStorage for SqliteStorage {
    type Config = SqliteConfig;

    async fn new(config: Self::Config) -> Result<Self> {
        let embedder = OpenAiEmbedder::default();
        let store = SqliteVectorStore::new(&config.path, embedder.clone()).await?;

        Ok(Self {
            store,
            embedder: Arc::new(embedder)
        })
    }

    async fn add_documents(&self, documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        let (docs, _metadata): (Vec<Document>, Vec<DocumentMetadata>) = documents.into_iter().unzip();
        self.store.add_documents(&docs, &VecStoreOptions::default()).await?;
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize) -> Result<Vec<(Document, f32)>> {
        self.store.similarity_search_with_score(query, limit, &VecStoreOptions::default()).await
    }

    async fn delete_documents(&self, _filter: MetadataFilter) -> Result<u64> {
        // Note: The SQLiteVectorStore doesn't currently support filtered deletion
        // We could implement this later if needed
        Ok(0)
    }

    async fn count(&self) -> Result<u64> {
        Ok(self.store.count().await? as u64)
    }
}

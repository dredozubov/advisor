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
        let embedder = OpenAiEmbedder::default();
        let store = SqliteVectorStore::create(&config.path).await?;

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
        let results = self.store.similarity_search(query, limit, &VecStoreOptions::default()).await?;
        Ok(results.into_iter().map(|doc| (doc, 1.0)).collect())
    }

    async fn delete_documents(&self, _filter: MetadataFilter) -> Result<u64> {
        // Note: The SQLiteVectorStore doesn't currently support filtered deletion
        // We could implement this later if needed
        Ok(0)
    }

    async fn count(&self) -> Result<u64> {
        let count = sqlx::query!("SELECT COUNT(*) as count FROM documents")
            .fetch_one(&self.store.pool)
            .await?;
        Ok(count.count as u64)
    }
}

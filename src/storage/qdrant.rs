use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{
    embedding::openai::OpenAiEmbedder,
    schemas::Document,
    vectorstore::{qdrant::{Qdrant, StoreBuilder}, VecStoreOptions},
};
use serde::{Deserialize, Serialize};

use super::{DocumentMetadata, MetadataFilter, VectorStorage};

#[derive(Debug, Clone)]
pub struct QdrantConfig {
    pub url: String,
    pub collection_name: String,
}

pub struct QdrantStorage {
    store: Qdrant,
    collection_name: String,
}

#[async_trait]
impl VectorStorage for QdrantStorage {
    type Config = QdrantConfig;

    async fn new(config: Self::Config) -> Result<Self> {
        let embedder = OpenAiEmbedder::default();
        let client = qdrant_client::Qdrant::from_url(&config.url).build()?;

        let store = StoreBuilder::new()
            .embedder(embedder)
            .client(client)
            .collection_name(&config.collection_name)
            .build()
            .await?;

        Ok(Self {
            store,
            collection_name: config.collection_name,
        })
    }

    async fn add_documents(&self, documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        let (docs, _metadata): (Vec<Document>, Vec<DocumentMetadata>) = documents.into_iter().unzip();
        self.store
            .add_documents(&docs, &VecStoreOptions::default())
            .await?;
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize) -> Result<Vec<(Document, f32)>> {
        let results = self.store
            .similarity_search_with_score(query, limit, &VecStoreOptions::default())
            .await?;
        Ok(results)
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

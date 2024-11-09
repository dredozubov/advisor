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
    async fn add_documents(&self, documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        let points = documents
            .into_iter()
            .map(|(doc, _meta)| qdrant_client::qdrant::PointStruct {
                id: None,
                vector: self.embedder.embed_document(&doc.page_content).await?,
                payload: None, // Add metadata if needed
            })
            .collect::<Vec<_>>();

        self.client
            .upsert_points(&self.collection_name, points)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to add documents to Qdrant: {}", e))?;

        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize) -> Result<Vec<(Document, f32)>> {
        let query_vector = self.embedder.embed_query(query).await?;
        let search_result = self
            .client
            .search_points(&self.collection_name, query_vector, limit)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to perform similarity search: {}", e))?;

        let documents = search_result
            .into_iter()
            .map(|point| {
                let doc = Document {
                    page_content: String::new(), // Retrieve actual content if needed
                    metadata: HashMap::new(),    // Add metadata if needed
                    score: point.score,
                };
                (doc, point.score)
            })
            .collect();

        Ok(documents)
    }

    async fn delete_documents(&self, _filter: MetadataFilter) -> Result<u64> {
        // Implement deletion logic if needed
        Ok(0)
    }

    async fn count(&self) -> Result<u64> {
        let count = self
            .client
            .count_points(&self.collection_name)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to count documents in Qdrant: {}", e))?;

        Ok(count as u64)
    }
}

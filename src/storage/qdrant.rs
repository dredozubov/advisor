use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{embedding::Embedder, schemas::Document, vectorstore::VectorStore};
use qdrant_client::config::QdrantConfig;


#[derive(Debug, Clone)]
pub struct QdrantStoreConfig {
    pub uri: String,
    pub collection_name: String,
}

use langchain_rust::vectorstore::{VecStoreOptions, VectorStore};

pub struct QdrantStorage {
    store: Qdrant,
    embedder: Arc<dyn Embedder>,
}

#[async_trait]
#[async_trait]
impl VectorStore for QdrantStorage {
    async fn add_documents(
        &self,
        documents: &[Document],
        _options: &VecStoreOptions,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let points = documents
            .iter()
            .map(|doc| {
                let vector = futures::executor::block_on(self.embedder.embed_document(&doc.page_content)).unwrap();
                qdrant_client::qdrant::PointStruct {
                    id: None,
                    vector,
                    payload: HashMap::new(), // Add metadata if needed
                }
            })
            .collect::<Vec<_>>();

        self.store.add_documents(documents, &_options).await?;

        Ok(())
    }

    async fn similarity_search(
        &self,
        query: &str,
        limit: usize,
        _options: &VecStoreOptions,
    ) -> Result<Vec<Document>, Box<dyn std::error::Error>> {
        let query_vector = self.embedder.embed_query(query).await?;
        let search_result = self
            .store
            .similarity_search(query, limit, &_options)
            .await?;

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

}

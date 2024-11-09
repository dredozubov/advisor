use async_trait::async_trait;
use langchain_rust::{
    embedding::Embedder,
    schemas::Document,
    vectorstore::{VecStoreOptions, VectorStore},
};
use std::error::Error as StdError;
use anyhow::Result;
use std::sync::{Arc, RwLock};

pub struct InMemoryStore {
    docs: RwLock<Vec<Document>>,
    embedder: Arc<dyn Embedder>,
}

impl InMemoryStore {
    pub fn new(embedder: Arc<dyn Embedder>) -> Self {
        Self {
            docs: RwLock::new(Vec::new()),
            embedder,
        }
    }

    async fn compute_similarity(v1: &[f32], v2: &[f32]) -> f32 {
        // Cosine similarity
        let dot_product: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        let norm1: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm2: f32 = v2.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot_product / (norm1 * norm2)
    }

}

#[async_trait]
impl VectorStore for InMemoryStore {
    async fn add_documents(&self, documents: &[Document], _options: &VecStoreOptions) -> Result<Vec<String>, Box<dyn StdError>> {
        // Get embeddings for documents
        let texts: Vec<_> = documents.iter().map(|d| d.page_content.clone()).collect();
        let embeddings = self.embedder.embed_documents(&texts).await?;
        
        // Create documents with embeddings
        let mut docs = self.docs.write().unwrap();
        for (doc, embedding) in documents.iter().zip(embeddings.iter()) {
            let mut new_doc = doc.clone();
            new_doc.metadata.insert("embedding".to_string(), serde_json::json!(embedding));
            docs.push(new_doc);
        }
        
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize, options: &VecStoreOptions) -> Result<Vec<Document>, Box<dyn StdError>> {
        // Get query embedding
        let query_embedding: Vec<f64> = self.embedder.embed_query(query).await?;
        let query_embedding: Vec<f32> = query_embedding.iter().map(|&x| x as f32).collect();
        
        // Search both memory and disk
        let memory_results = {
            let docs = self.docs.read().unwrap();
            let mut scored_docs: Vec<(f32, Document)> = Vec::new();
            
            for doc in docs.iter() {
                if let Some(embedding) = doc.metadata.get("embedding") {
                    let doc_embedding: Vec<f32> = serde_json::from_value(embedding.clone())
                        .map_err(|e| Box::new(e) as Box<dyn StdError>)?;
                    let similarity = Self::compute_similarity(&query_embedding, &doc_embedding).await;
                    scored_docs.push((similarity, doc.clone()));
                }
            }
            
            scored_docs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            Ok(scored_docs.into_iter()
            .take(limit)
            .map(|(score, mut doc)| {
                doc.score = score as f64;
                doc
            })
            .collect())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use langchain_rust::embedding::fake::FakeEmbedder;

    #[tokio::test]
    async fn test_add_and_search() {
        let store = InMemoryStore::new(Arc::new(FakeEmbedder::new()));

        let docs = vec![
            Document {
                page_content: "Hello world".to_string(),
                metadata: Default::default(),
                score: 0.0,
            },
            Document {
                page_content: "Goodbye world".to_string(),
                metadata: Default::default(),
                score: 0.0,
            },
        ];

        store.add_documents(&docs, &VecStoreOptions::default()).await.unwrap();
        
        let results = store.similarity_search(
            "hello",
            1,
            &VecStoreOptions::default()
        ).await.unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].score > 0.0);
    }
}

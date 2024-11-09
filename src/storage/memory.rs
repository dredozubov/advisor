use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{
    embedder::Embedder,
    schemas::Document,
    vectorstore::{VecStoreOptions, VectorStore},
};
use std::sync::{Arc, RwLock};

pub struct InMemoryStore {
    documents: RwLock<Vec<Document>>,
    embedder: Arc<dyn Embedder>,
}

impl InMemoryStore {
    pub fn new(embedder: Arc<dyn Embedder>) -> Self {
        Self {
            documents: RwLock::new(Vec::new()),
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
    async fn add_documents(&self, documents: &[Document], options: &VecStoreOptions) -> Result<()> {
        // Get embeddings for documents
        let texts: Vec<String> = documents.iter().map(|d| d.page_content.clone()).collect();
        let embeddings = self.embedder.embed_texts(&texts).await?;
        
        // Create documents with embeddings
        let mut docs = self.documents.write().unwrap();
        for (doc, embedding) in documents.iter().zip(embeddings.iter()) {
            let mut new_doc = doc.clone();
            new_doc.metadata.insert("embedding".to_string(), serde_json::json!(embedding));
            docs.push(new_doc);
        }
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize, _options: &VecStoreOptions) -> Result<Vec<Document>> {
        // Get query embedding
        let query_embedding = self.embedder.embed_text(query).await?;
        
        // Get all documents and their embeddings
        let docs = self.documents.read().unwrap();
        
        // Calculate similarities and sort
        let mut scored_docs: Vec<(f32, Document)> = Vec::new();
        for doc in docs.iter() {
            if let Some(embedding) = doc.metadata.get("embedding") {
                let doc_embedding: Vec<f32> = serde_json::from_value(embedding.clone())?;
                let similarity = Self::compute_similarity(&query_embedding, &doc_embedding).await;
                scored_docs.push((similarity, doc.clone()));
            }
        }
        
        // Sort by similarity (descending)
        scored_docs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        
        // Return top k documents
        Ok(scored_docs.into_iter()
            .take(limit)
            .map(|(score, mut doc)| {
                doc.score = score;
                doc
            })
            .collect())
    }

    async fn delete_documents(&self, _filter: &str) -> Result<()> {
        // Optional: Implement deletion based on metadata filter
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use langchain_rust::embedding::fake::FakeEmbedder;

    #[tokio::test]
    async fn test_add_and_search() {
        let embedder = Arc::new(FakeEmbedder::new());
        let store = InMemoryStore::new(embedder);

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

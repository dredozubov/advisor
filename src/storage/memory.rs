use crate::storage::VectorStorage;
use async_trait::async_trait;
use langchain_rust::{embedding::Embedder, schemas::Document, vectorstore::VecStoreOptions};
use std::error::Error as StdError;
use std::sync::{Arc, RwLock};
pub struct InMemoryStore {
    docs: RwLock<Vec<Document>>,
    embedder: Arc<dyn Embedder>,
}

async fn compute_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    let dot_product: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
    let norm1: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm2: f32 = v2.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot_product / (norm1 * norm2)
}

#[async_trait]
impl VectorStorage for InMemoryStore {
    fn is_ephemeral(&self) -> bool {
        true
    }

    async fn add_documents(
        &self,
        documents: &[Document],
        _options: &VecStoreOptions,
    ) -> Result<Vec<String>, Box<dyn StdError>> {
        let texts: Vec<_> = documents.iter().map(|d| d.page_content.clone()).collect();
        let embeddings = self.embedder.embed_documents(&texts).await?;

        let mut docs = self.docs.write().unwrap();
        for (doc, embedding) in documents.iter().zip(embeddings.iter()) {
            let mut new_doc = doc.clone();
            new_doc
                .metadata
                .insert("embedding".to_string(), serde_json::json!(embedding));
            log::debug!("Generated embedding for document: {:?}", doc.page_content);
            log::debug!("Embedding: {:?}", embedding);
            docs.push(new_doc);
        }

        Ok(texts)
    }

    async fn similarity_search(
        &self,
        query: &str,
        limit: usize,
        _options: &VecStoreOptions,
    ) -> Result<Vec<Document>, Box<dyn StdError>> {
        let query_embedding: Vec<f64> = self.embedder.embed_query(query).await?;
        let query_embedding: Vec<f32> = query_embedding.iter().map(|&x| x as f32).collect();
        log::debug!("Query embedding: {:?}", query_embedding);

        // Collect all documents and embeddings before processing
        let docs_with_embeddings = {
            let docs = self.docs.read().unwrap();
            docs.iter()
                .filter_map(|doc| {
                    doc.metadata
                        .get("embedding")
                        .map(|e| (doc.clone(), e.clone()))
                })
                .collect::<Vec<_>>()
        };

        // Process embeddings and compute similarities
        let mut scored_docs = Vec::new();
        for (doc, embedding) in docs_with_embeddings {
            let doc_embedding: Vec<f32> =
                serde_json::from_value(embedding).map_err(|e| Box::new(e) as Box<dyn StdError>)?;
            let similarity = Self::compute_similarity(&query_embedding, &doc_embedding).await;
            log::debug!(
                "Similarity score for document '{}': {}",
                doc.page_content,
                similarity
            );
            scored_docs.push((similarity, doc));
        }

        scored_docs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        Ok(scored_docs
            .into_iter()
            .take(limit)
            .map(|(score, mut doc)| {
                doc.score = score as f64;
                doc
            })
            .collect())
    }
}

#[cfg(test)]
mod fake {
    use async_trait::async_trait;
    use langchain_rust::embedding::Embedder;

    pub struct FakeEmbedder;

    impl FakeEmbedder {
        pub fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl Embedder for FakeEmbedder {
        async fn embed_documents(
            &self,
            texts: &[String],
        ) -> Result<Vec<Vec<f64>>, langchain_rust::embedding::EmbedderError> {
            Ok(texts.iter().map(|_| vec![0.5, 0.5]).collect())
        }

        async fn embed_query(
            &self,
            _text: &str,
        ) -> Result<Vec<f64>, langchain_rust::embedding::EmbedderError> {
            Ok(vec![0.5, 0.5])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeEmbedder;
    use super::*;

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

        store
            .add_documents(&docs, &VecStoreOptions::default())
            .await
            .unwrap();

        let results = store
            .similarity_search("hello", 1, &VecStoreOptions::default())
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].score > 0.0);
    }
}

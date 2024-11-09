use async_trait::async_trait;
use langchain_rust::{
    embedding::Embedder,
    schemas::Document,
    vectorstore::{VecStoreOptions, VectorStore},
};
use std::error::Error as StdError;
use anyhow::Result;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

const DEFAULT_MAX_MEMORY_DOCS: usize = 10_000;
const DEFAULT_CACHE_TTL_SECS: u64 = 3600; // 1 hour

#[derive(Debug)]
struct CachedDocument {
    document: Document,
    last_accessed: SystemTime,
}

pub struct InMemoryStoreConfig {
    max_memory_docs: usize,
    cache_ttl_secs: u64,
}

impl Default for InMemoryStoreConfig {
    fn default() -> Self {
        Self {
            max_memory_docs: DEFAULT_MAX_MEMORY_DOCS,
            cache_ttl_secs: DEFAULT_CACHE_TTL_SECS,
        }
    }
}

pub struct InMemoryStore {
    memory_docs: RwLock<VecDeque<CachedDocument>>,
    disk_store: Arc<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    config: InMemoryStoreConfig,
}

impl InMemoryStore {
    pub fn new(embedder: Arc<dyn Embedder>, disk_store: Arc<dyn VectorStore>, config: Option<InMemoryStoreConfig>) -> Self {
        Self {
            memory_docs: RwLock::new(VecDeque::new()),
            disk_store,
            embedder,
            config: config.unwrap_or_default(),
        }
    }

    async fn compute_similarity(v1: &[f32], v2: &[f32]) -> f32 {
        // Cosine similarity
        let dot_product: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        let norm1: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm2: f32 = v2.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot_product / (norm1 * norm2)
    }

    async fn evict_old_documents(&self) -> Result<()> {
        let mut docs = self.memory_docs.write().unwrap();
        let now = SystemTime::now();
        
        // Remove documents that exceed TTL
        while let Some(doc) = docs.front() {
            if doc.last_accessed.elapsed().unwrap().as_secs() > self.config.cache_ttl_secs {
                if let Some(evicted) = docs.pop_front() {
                    // Move to disk store
                    self.disk_store.add_documents(&[evicted.document], &VecStoreOptions::default()).await?;
                }
            } else {
                break;
            }
        }

        // If still over capacity, remove oldest
        while docs.len() > self.config.max_memory_docs {
            if let Some(evicted) = docs.pop_front() {
                self.disk_store.add_documents(&[evicted.document], &VecStoreOptions::default()).await?;
            }
        }

        Ok(texts)
    }

    async fn update_access_time(&self, doc: &Document) {
        if let Some(cached_doc) = self.memory_docs.write().unwrap().iter_mut()
            .find(|d| d.document.page_content == doc.page_content) {
            cached_doc.last_accessed = SystemTime::now();
        }
    }
}

#[async_trait]
impl VectorStore for InMemoryStore {
    async fn add_documents(&self, documents: &[Document], _options: &VecStoreOptions) -> Result<Vec<String>, Box<dyn StdError>> {
        // Get embeddings for documents
        let texts: Vec<_> = documents.iter().map(|d| d.page_content.clone()).collect();
        let embeddings = self.embedder.embed_documents(&texts).await?;
        
        // Create documents with embeddings
        let mut docs = self.memory_docs.write().unwrap();
        for (doc, embedding) in documents.iter().zip(embeddings.iter()) {
            let mut new_doc = doc.clone();
            new_doc.metadata.insert("embedding".to_string(), serde_json::json!(embedding));
            
            docs.push_back(CachedDocument {
                document: new_doc,
                last_accessed: SystemTime::now(),
            });
        }

        // Evict old documents if necessary
        self.evict_old_documents().await?;
        
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize, options: &VecStoreOptions) -> Result<Vec<Document>, Box<dyn StdError>> {
        // Get query embedding
        let query_embedding: Vec<f64> = self.embedder.embed_query(query).await?;
        let query_embedding: Vec<f32> = query_embedding.iter().map(|&x| x as f32).collect();
        
        // Search both memory and disk
        let memory_results = {
            let docs = self.memory_docs.read().unwrap();
            let mut scored_docs: Vec<(f32, Document)> = Vec::new();
            
            for cached_doc in docs.iter() {
                if let Some(embedding) = cached_doc.document.metadata.get("embedding") {
                    let doc_embedding: Vec<f32> = serde_json::from_value(embedding.clone())
                        .map_err(|e| Box::new(e) as Box<dyn StdError>)?;
                    let similarity = Self::compute_similarity(&query_embedding, &doc_embedding).await;
                    scored_docs.push((similarity, cached_doc.document.clone()));
                }
            }
            
            scored_docs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            scored_docs
        };

        // Get results from disk store
        let disk_results = self.disk_store.similarity_search(query, limit, options).await?;

        // Merge and sort results
        let mut all_results: Vec<(f32, Document)> = memory_results;
        all_results.extend(disk_results.into_iter().map(|doc| (doc.score as f32, doc)));
        all_results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        // Update access times for returned memory documents
        for (_, doc) in all_results.iter() {
            self.update_access_time(doc).await;
        }

        // Return top k documents
        Ok(all_results.into_iter()
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
        let embedder = Arc::new(FakeEmbedder::new());
        let disk_store = Arc::new(InMemoryStore::new(
            embedder.clone(),
            Arc::new(FakeEmbedder::new()) as Arc<dyn VectorStore>,
            None,
        ));
        let store = InMemoryStore::new(embedder, disk_store, None);

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

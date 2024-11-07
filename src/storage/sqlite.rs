use crate::storage::{DocumentMetadata, MetadataFilter, VectorStorage};
use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{
    embedding::openai::OpenAiEmbedder,
    schemas::Document,
    vectorstore::{sqlite::SqliteStore, Store, StoreOptions},
};

#[derive(Debug, Clone)]
pub struct SqliteConfig {
    pub path: String,
}

pub struct SqliteStorage {
    client: Arc<qdrant_client::Qdrant>,
    embedder: Arc<OpenAiEmbedder>,
}

#[async_trait]
impl VectorStorage for SqliteStorage {
    type Config = SqliteConfig;

    async fn new(config: Self::Config) -> Result<Self> {
        let embedder = OpenAiEmbedder::default();
        let client = qdrant_client::Qdrant::from_url(&config.url).build()?;

        Ok(Self {
            client: Arc::new(client),
            embedder: Arc::new(embedder)
        })
    }

    async fn add_documents(&self, documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        let (docs, metadata): (Vec<Document>, Vec<DocumentMetadata>) = documents.into_iter().unzip();
        let embeddings = self.embedder.embed_documents(&docs).await?;
        
        // Convert documents and embeddings to points
        let points: Vec<_> = docs.iter().zip(embeddings.iter()).map(|(doc, embedding)| {
            qdrant_client::qdrant::PointStruct {
                id: None, // Let Qdrant generate IDs
                payload: doc.metadata.clone(),
                vectors: Some(embedding.clone()),
            }
        }).collect();

        self.client
            .upsert_points("documents", points, None)
            .await?;
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize) -> Result<Vec<(Document, f32)>> {
        let query_embedding = self.embedder.embed_query(query).await?;
        
        let search_result = self.client
            .search_points(&qdrant_client::qdrant::SearchPoints {
                collection_name: "documents".to_string(),
                vector: query_embedding,
                limit: limit as u64,
                ..Default::default()
            })
            .await?;

        let results = search_result.result
            .into_iter()
            .map(|point| {
                let doc = Document {
                    page_content: point.payload.get("text").unwrap().to_string(),
                    metadata: point.payload,
                };
                (doc, point.score)
            })
            .collect();

        Ok(results)
    }

    async fn delete_documents(&self, filter: MetadataFilter) -> Result<u64> {
        let mut conditions = Vec::new();
        
        if let Some(source) = filter.source {
            conditions.push(format!("metadata->>'source' = '{}'", source));
        }
        if let Some(report_type) = filter.report_type {
            conditions.push(format!("metadata->>'report_type' = '{}'", report_type));
        }
        if let Some(ticker) = filter.ticker {
            conditions.push(format!("metadata->>'ticker' = '{}'", ticker));
        }
        if let Some((start, end)) = filter.date_range {
            conditions.push(format!(
                "metadata->>'filing_date' BETWEEN '{}' AND '{}'",
                start, end
            ));
        }

        let where_clause = if conditions.is_empty() {
            "TRUE".to_string()
        } else {
            conditions.join(" AND ")
        };

        self.store.delete_documents(&where_clause).await
    }

    async fn count(&self) -> Result<u64> {
        self.store.count().await
    }
}

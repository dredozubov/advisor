use crate::storage::{DocumentMetadata, MetadataFilter, VectorStorage};
use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{
    embedding::openai::OpenAiEmbedder,
    schemas::Document,
    store::{Store, StoreOptions},
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SqliteConfig {
    pub path: String,
}

pub struct SqliteStorage {
    store: Arc<Store>,
}

#[async_trait]
impl VectorStorage for SqliteStorage {
    type Config = SqliteConfig;

    async fn new(config: Self::Config) -> Result<Self> {
        let embedder = OpenAiEmbedder::default();
        let store = Store::new()
            .with_embedder(embedder)
            .with_sqlite(&config.path)
            .with_dimensions(1536)
            .build()
            .await?;

        Ok(Self {
            store: Arc::new(store)
        })
    }

    async fn add_documents(&self, documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        let (docs, metadata): (Vec<Document>, Vec<DocumentMetadata>) = documents.into_iter().unzip();
        
        for (doc, meta) in docs.iter().zip(metadata.iter()) {
            let options = StoreOptions::default()
                .with_metadata(serde_json::json!({
                    "source": meta.source,
                    "report_type": meta.report_type,
                    "filing_date": meta.filing_date,
                    "company_name": meta.company_name,
                    "ticker": meta.ticker,
                }));
            self.store.add_document(doc, &options).await?;
        }
        
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize) -> Result<Vec<(Document, f32)>> {
        self.store.similarity_search(query, limit).await
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

use crate::storage::{DocumentMetadata, MetadataFilter, VectorStorage};
use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{
    embedding::openai::OpenAiEmbedder,
    schemas::Document,
    vectorstore::{sqlite_vss::StoreBuilder, VecStoreOptions, VectorStore},
};
use serde_json::json;

pub struct SqliteStorage {
    store: Box<dyn VectorStore>,
}

#[async_trait]
impl VectorStorage for SqliteStorage {
    type Config = SqliteConfig;

    async fn new(config: Self::Config) -> Result<Self> {
        let embedder = OpenAiEmbedder::default();
        
        let store = StoreBuilder::new()
            .embedder(embedder)
            .connection_url(config.path)
            .table("documents")
            .vector_dimensions(1536)
            .build()
            .await?;

        // Initialize tables if they don't exist
        store.initialize().await?;

        Ok(Self {
            store: Box::new(store)
        })
    }

    async fn add_documents(&self, documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        let (docs, metadata): (Vec<Document>, Vec<DocumentMetadata>) = documents.into_iter().unzip();
        
        // Convert metadata to VecStoreOptions
        let options = metadata.into_iter().map(|m| {
            VecStoreOptions::default().with_metadata(json!({
                "source": m.source,
                "report_type": m.report_type,
                "filing_date": m.filing_date,
                "company_name": m.company_name,
                "ticker": m.ticker,
            }))
        }).collect::<Vec<_>>();

        for (doc, opt) in docs.iter().zip(options.iter()) {
            self.store.add_documents(&[doc.clone()], opt).await?;
        }
        
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize) -> Result<Vec<(Document, f32)>> {
        self.store.similarity_search_with_score(query, limit, &VecStoreOptions::default()).await
    }

    async fn delete_documents(&self, filter: MetadataFilter) -> Result<u64> {
        // Convert filter to SQL WHERE clause
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

        // Execute delete query
        // Note: This is a simplified version - you'd want to implement this properly
        // using the actual SQLite store API
        todo!("Implement delete_documents")
    }

    async fn count(&self) -> Result<u64> {
        // Note: This is a simplified version - you'd want to implement this properly
        // using the actual SQLite store API
        todo!("Implement count")
    }
}

use std::sync::Arc;
use crate::storage::{DocumentMetadata, MetadataFilter, VectorStorage};
use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::{
    embedding::openai::OpenAiEmbedder,
    schemas::Document,
};
use sqlx::sqlite::SqlitePool;

#[derive(Debug, Clone)]
pub struct SqliteConfig {
    pub path: String,
}

pub struct SqliteStorage {
    pool: Arc<SqlitePool>,
    embedder: Arc<OpenAiEmbedder>,
}

#[async_trait]
impl VectorStorage for SqliteStorage {
    type Config = SqliteConfig;

    async fn new(config: Self::Config) -> Result<Self> {
        let embedder = OpenAiEmbedder::default();
        let pool = SqlitePool::connect(&config.path).await?;

        Ok(Self {
            pool: Arc::new(pool),
            embedder: Arc::new(embedder)
        })
    }

    async fn add_documents(&self, documents: Vec<(Document, DocumentMetadata)>) -> Result<()> {
        let (docs, metadata): (Vec<Document>, Vec<DocumentMetadata>) = documents.into_iter().unzip();
        let embeddings = self.embedder.embed_documents(&docs).await?;
        
        for ((doc, meta), embedding) in docs.into_iter().zip(metadata).zip(embeddings) {
            sqlx::query!(
                "INSERT INTO documents (content, metadata, embedding) VALUES (?, ?, ?)",
                doc.page_content,
                serde_json::to_string(&meta)?,
                embedding.to_vec()
            )
            .execute(&*self.pool)
            .await?;
        }
        Ok(())
    }

    async fn similarity_search(&self, query: &str, limit: usize) -> Result<Vec<(Document, f32)>> {
        let query_embedding = self.embedder.embed_query(query).await?;
        
        let rows = sqlx::query!(
            r#"
            SELECT content, metadata, 
                   (embedding <=> $1) as distance
            FROM documents 
            ORDER BY distance ASC
            LIMIT $2
            "#,
            query_embedding.to_vec(),
            limit as i64
        )
        .fetch_all(&*self.pool)
        .await?;

        let results = rows.into_iter()
            .map(|row| {
                let metadata: DocumentMetadata = serde_json::from_str(&row.metadata)?;
                let doc = Document {
                    page_content: row.content,
                    metadata: serde_json::to_value(metadata)?,
                    score: Some(row.distance as f32),
                };
                Ok((doc, row.distance as f32))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(results)
    }

    async fn delete_documents(&self, filter: MetadataFilter) -> Result<u64> {
        let mut conditions = Vec::new();
        let mut params = Vec::new();
        
        if let Some(source) = filter.source {
            conditions.push("json_extract(metadata, '$.source') = ?");
            params.push(source);
        }
        if let Some(report_type) = filter.report_type {
            conditions.push("json_extract(metadata, '$.report_type') = ?");
            params.push(report_type.to_string());
        }
        if let Some(ticker) = filter.ticker {
            conditions.push("json_extract(metadata, '$.ticker') = ?");
            params.push(ticker);
        }
        if let Some((start, end)) = filter.date_range {
            conditions.push("json_extract(metadata, '$.filing_date') BETWEEN ? AND ?");
            params.push(start.to_string());
            params.push(end.to_string());
        }

        let where_clause = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" AND ")
        };

        let result = sqlx::query(&format!("DELETE FROM documents WHERE {}", where_clause))
            .execute(&*self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    async fn count(&self) -> Result<u64> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM documents")
            .fetch_one(&*self.pool)
            .await?;
        Ok(result.count as u64)
    }
}

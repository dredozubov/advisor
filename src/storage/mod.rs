use std::sync::Arc;

use crate::edgar::report;
use anyhow::Result;
use async_trait::async_trait;
use langchain_rust::embedding::openai::OpenAiEmbedder;
use langchain_rust::embedding::{Embedder, EmbedderError};
use langchain_rust::llm::OpenAIConfig;
use langchain_rust::schemas::Document;
use serde::{Deserialize, Serialize};

/// Metadata associated with stored documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub source: String,                          // "edgar", "earnings", etc.
    pub report_type: Option<report::ReportType>, // For SEC filings
    pub filing_date: Option<chrono::NaiveDate>,
    pub company_name: Option<String>,
    pub ticker: Option<String>,
}

/// Filter for querying documents by metadata
#[derive(Debug, Clone)]
pub struct MetadataFilter {
    pub source: Option<String>,
    pub report_type: Option<report::ReportType>,
    pub ticker: Option<String>,
    pub date_range: Option<(chrono::NaiveDate, chrono::NaiveDate)>,
    pub custom: Option<serde_json::Value>,
}

/// Core trait that must be implemented by all storage backends
#[async_trait]
pub trait VectorStorage {
    /// Configuration type specific to this storage implementation
    type Config;

    /// Initialize the storage backend with implementation-specific configuration
    async fn new(config: Self::Config, embedder: EmbedderWrapper) -> Result<Self>
    where
        Self: Sized;

    /// Add documents with optional metadata
    async fn add_documents(&self, documents: Vec<(Document, DocumentMetadata)>) -> Result<()>;

    /// Perform similarity search
    async fn similarity_search(&self, query: &str, limit: usize) -> Result<Vec<(Document, f32)>>;

    /// Delete documents by metadata filter
    async fn delete_documents(&self, filter: MetadataFilter) -> Result<u64>;

    /// Get document count
    async fn count(&self) -> Result<u64>;
}

#[derive(Debug, Clone)]
pub enum EmbedderWrapper {
    OpenAiEmbedderWrapper(OpenAiEmbedder<OpenAIConfig>),
}

impl Embedder for EmbedderWrapper {
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    fn embed_documents<'life0, 'life1, 'async_trait>(
        &'life0 self,
        documents: &'life1 [String],
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<Vec<Vec<f64>>, EmbedderError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        match self {
            EmbedderWrapper::OpenAiEmbedderWrapper(open_ai_embedder) => {
                open_ai_embedder.embed_documents(documents)
            }
        }
    }

    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    fn embed_query<'life0, 'life1, 'async_trait>(
        &'life0 self,
        text: &'life1 str,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<Vec<f64>, EmbedderError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        match self {
            EmbedderWrapper::OpenAiEmbedderWrapper(open_ai_embedder) => {
                open_ai_embedder.embed_query(text)
            }
        }
    }
}

pub mod qdrant;
pub mod sqlite;

// Re-export storage implementations
pub use self::qdrant::{QdrantStorage, QdrantStoreConfig};
pub use self::sqlite::{SqliteConfig, SqliteStorage};

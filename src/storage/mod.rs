use crate::edgar::report;
use anyhow::Result;
use async_trait::async_trait;
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
    async fn new(config: Self::Config) -> Result<Self>
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

pub mod qdrant;
pub mod sqlite;

// Re-export storage implementations
pub use self::qdrant::{QdrantStoreConfig, QdrantStorage}; 
pub use self::sqlite::{SqliteConfig, SqliteStorage};

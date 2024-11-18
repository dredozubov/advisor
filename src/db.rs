use anyhow::Result;
use serde_json::Value;
use sqlx::{query, Pool, Postgres};

pub const COLLECTIONS_TABLE: &str = "vs_collections";
pub const EMBEDDER_TABLE: &str = "vs_embeddings";

/// Struct to represent metadata filter criteria
pub struct MetadataFilter {
    pub key: String,
    pub value: String,
}

/// Function to count vectors in the COLLECTIONS_TABLE where metadata matches the filter
pub async fn count_vectors_with_metadata(
    pool: &Pool<Postgres>,
    filter: MetadataFilter,
) -> Result<i64> {
    let query_str = format!(
        "SELECT COUNT(*) FROM {} WHERE cmetadata->> $1 = $2",
        COLLECTIONS_TABLE
    );
    let count: (i64,) = query(&query_str)
        .bind(filter.key)
        .bind(filter.value)
        .fetch_one(pool)
        .await?;
    Ok(count.0)
}

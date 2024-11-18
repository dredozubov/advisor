use anyhow::Result;
use serde_json::Value;
use sqlx::{query, Pool, Postgres};

pub const COLLECTIONS_TABLE: &str = "vs_collections";
pub const EMBEDDER_TABLE: &str = "vs_embeddings";

pub async fn count_vectors_with_filepath(pool: &Pool<Postgres>, filepath: &str) -> Result<i64> {
    let query_str = format!(
        "SELECT COUNT(*) FROM {} WHERE cmetadata->> 'filepath' = $1",
        COLLECTIONS_TABLE
    );
    let count = query(&query_str).bind(filepath).fetch_one(pool).await?;
    Ok(count)
}

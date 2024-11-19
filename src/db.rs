use anyhow::Result;
use serde_json::Value;
use sqlx::{query, query_as, Pool, Postgres};

pub const COLLECTIONS_TABLE: &str = "vs_collections";
pub const EMBEDDER_TABLE: &str = "vs_embeddings";

pub async fn count_vectors_with_filepath(pool: &Pool<Postgres>, filepath: &str) -> Result<i64> {
    let query_str = format!(
        "SELECT COUNT(*) FROM {} WHERE cmetadata->> 'filepath' = $1",
        COLLECTIONS_TABLE
    );
    let row: (i64,) = query_as(&query_str).bind(filepath).fetch_one(pool).await?;
    Ok(row.0)
}

pub async fn insert_memory(pool: &Pool<Postgres>, file_path: &str, messages: &Value) -> Result<()> {
    query("INSERT INTO memory (file_path, messages) VALUES ($1, $2)")
        .bind(file_path)
        .bind(messages)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_memory(pool: &Pool<Postgres>, file_path: &str, messages: &Value) -> Result<()> {
    query("UPDATE memory SET messages = $1, updated_at = NOW() WHERE file_path = $2")
        .bind(messages)
        .bind(file_path)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_memory(pool: &Pool<Postgres>, file_path: &str) -> Result<Option<Value>> {
    let result = query("SELECT messages FROM memory WHERE file_path = $1")
        .bind(file_path)
        .fetch_optional(pool)
        .await?;

    Ok(result.map(|r| r.0))
}

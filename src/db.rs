use anyhow::Result;
use sqlx::{query_as, Pool, Postgres};

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
use sqlx::{query, query_as, Pool, Postgres};
use serde_json::Value;
use anyhow::Result;

pub async fn insert_memory(pool: &Pool<Postgres>, file_path: &str, messages: &Value) -> Result<()> {
    query!(
        "INSERT INTO memory (file_path, messages) VALUES ($1, $2)",
        file_path,
        messages
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_memory(pool: &Pool<Postgres>, file_path: &str, messages: &Value) -> Result<()> {
    query!(
        "UPDATE memory SET messages = $2, updated_at = NOW() WHERE file_path = $1",
        file_path,
        messages
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_memory(pool: &Pool<Postgres>, file_path: &str) -> Result<Option<Value>> {
    let result = query_as!(
        (Value,),
        "SELECT messages FROM memory WHERE file_path = $1",
        file_path
    )
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|r| r.0))
}

use anyhow::Result;
use serde_json::Value;
use sqlx::{query, query_as, Pool, Postgres};
use uuid::Uuid;
use crate::memory::{Conversation, Message, MessageRole};

pub const COLLECTIONS_TABLE: &str = "vs_collections";
pub const EMBEDDER_TABLE: &str = "vs_embeddings";

// Conversation Database Operations
pub async fn get_most_recent_conversation(pool: &Pool<Postgres>) -> Result<Option<Conversation>> {
    sqlx::query_as!(
        Conversation,
        "SELECT id, summary, created_at, updated_at, tickers 
         FROM conversations 
         ORDER BY updated_at DESC 
         LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn create_conversation(
    pool: &Pool<Postgres>,
    summary: String,
    tickers: Vec<String>,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO conversations (id, summary, tickers, created_at, updated_at) 
         VALUES ($1, $2, $3, NOW(), NOW())",
        id,
        summary,
        &tickers
    )
    .execute(pool)
    .await?;

    Ok(id)
}

pub async fn update_conversation_summary(
    pool: &Pool<Postgres>,
    id: &Uuid,
    summary: String,
) -> Result<()> {
    sqlx::query!(
        "UPDATE conversations SET summary = $1, updated_at = NOW() WHERE id = $2",
        summary,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_conversation(pool: &Pool<Postgres>, id: &Uuid) -> Result<Option<Conversation>> {
    sqlx::query_as!(
        Conversation,
        "SELECT id, summary, created_at, updated_at, tickers 
         FROM conversations WHERE id = $1",
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn list_conversations(pool: &Pool<Postgres>) -> Result<Vec<Conversation>> {
    sqlx::query_as!(
        Conversation,
        "SELECT id, summary, created_at, updated_at, tickers 
         FROM conversations 
         ORDER BY updated_at DESC"
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn update_conversation_timestamp(pool: &Pool<Postgres>, id: &Uuid) -> Result<()> {
    sqlx::query!(
        "UPDATE conversations SET updated_at = NOW() WHERE id = $1",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

// Message Database Operations
pub async fn add_message(
    pool: &Pool<Postgres>,
    conversation_id: &Uuid,
    role: MessageRole,
    content: String,
    metadata: Value,
) -> Result<String> {
    let id = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO conversation_messages (id, conversation_id, role, content, metadata) 
         VALUES ($1, $2, $3, $4, $5)",
        id,
        conversation_id,
        role.to_string().to_lowercase(),
        content,
        metadata
    )
    .execute(pool)
    .await?;

    Ok(id.to_string())
}

pub async fn get_conversation_messages(
    pool: &Pool<Postgres>,
    conversation_id: &Uuid,
    limit: i64,
) -> Result<Vec<Message>> {
    sqlx::query_as!(
        Message,
        r#"
        SELECT 
            id,
            conversation_id,
            role,
            content,
            created_at,
            metadata
        FROM conversation_messages 
        WHERE conversation_id = $1 
        ORDER BY created_at DESC 
        LIMIT $2
        "#,
        conversation_id,
        limit
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn clear_conversation_messages(pool: &Pool<Postgres>, conversation_id: &Uuid) -> Result<()> {
    sqlx::query!(
        "DELETE FROM conversation_messages WHERE conversation_id = $1",
        conversation_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

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
    let result: Option<(Value,)> = query_as("SELECT messages FROM memory WHERE file_path = $1")
        .bind(file_path)
        .fetch_optional(pool)
        .await?;
    match result {
        Some(row) => Ok(Some(row.0)),
        None => Ok(None),
    }
}

pub async fn get_pool() -> Result<Pool<Postgres>> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set"))?;

    sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .connect(&database_url)
        .await
        .map_err(Into::into)
}

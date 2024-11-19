use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPool, types::Uuid};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tickers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub metadata: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

pub struct ConversationManager {
    pool: PgPool,
    current_conversation: Option<String>,
}

impl ConversationManager {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            current_conversation: None,
        }
    }

    pub async fn create_conversation(
        &mut self,
        title: String,
        tickers: Vec<String>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query!(
            "INSERT INTO conversations (id, title, tickers, created_at, updated_at) 
             VALUES ($1, $2, $3, NOW(), NOW())",
            id,
            title,
            &tickers
        )
        .execute(&self.pool)
        .await?;

        self.current_conversation = Some(id.clone());
        Ok(id)
    }

    pub async fn update_summary(&mut self, id: &str, summary: String) -> Result<()> {
        sqlx::query!(
            "UPDATE conversations SET summary = $1, updated_at = NOW() WHERE id = $2",
            summary,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_conversation(&self, id: &str) -> Result<Option<Conversation>> {
        sqlx::query_as!(
            Conversation,
            "SELECT id, title, summary, created_at, updated_at, tickers 
             FROM conversations WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn list_conversations(&self) -> Result<Vec<Conversation>> {
        sqlx::query_as!(
            Conversation,
            "SELECT id, title, summary, created_at, updated_at, tickers 
             FROM conversations 
             ORDER BY updated_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn switch_conversation(&mut self, id: String) -> Result<()> {
        if self.get_conversation(&id).await?.is_some() {
            self.current_conversation = Some(id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Conversation not found"))
        }
    }

    pub async fn get_current_conversation_details(&self) -> Result<Option<Conversation>> {
        if let Some(id) = &self.current_conversation {
            self.get_conversation(id).await
        } else {
            Ok(None)
        }
    }
}

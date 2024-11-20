use anyhow::Result;
use async_trait::async_trait;
use core::fmt;
use langchain_rust::{
    chain::{builder::ConversationalChainBuilder, ConversationalChain},
    llm::{OpenAI, OpenAIConfig},
    schemas::BaseMemory,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{postgres::PgPool, types::Uuid};
use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    sync::Arc,
};
use time::OffsetDateTime;

#[derive(Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub summary: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub tickers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,
    pub created_at: OffsetDateTime,
    pub metadata: Value,
}

pub struct ConversationChainManager {
    pool: PgPool,
    chains: HashMap<String, ConversationalChain>,
}

impl ConversationChainManager {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            chains: HashMap::new(),
        }
    }

    pub async fn get_or_create_chain(
        &mut self,
        conversation_id: &Uuid,
        llm: OpenAI<OpenAIConfig>,
    ) -> Result<&ConversationalChain> {
        let id_str = conversation_id.to_string();
        if !self.chains.contains_key(&id_str) {
            // Create database-backed memory
            let memory = DatabaseMemory::new(self.pool.clone(), *conversation_id);

            // Create new chain with database memory
            let chain = ConversationalChainBuilder::new()
                .llm(llm.clone())
                .memory(Arc::new(tokio::sync::Mutex::new(memory)))
                .build()?;

            self.chains.insert(id_str.clone(), chain);
        }

        Ok(&self.chains[&id_str])
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl From<String> for MessageRole {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            _ => MessageRole::User,
        }
    }
}

impl Display for MessageRole {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

// Database-backed memory implementation
pub struct DatabaseMemory {
    pool: PgPool,
    conversation_id: Uuid,
}

impl DatabaseMemory {
    pub fn new(pool: PgPool, conversation_id: Uuid) -> Self {
        Self {
            pool,
            conversation_id,
        }
    }
}

#[async_trait]
impl BaseMemory for DatabaseMemory {
    fn messages(&self) -> Vec<langchain_rust::schemas::Message> {
        // Return empty for now since we handle history differently
        vec![]
    }

    fn add_message(&mut self, message: langchain_rust::schemas::Message) {
        let pool = self.pool.clone();
        let conversation_id = self.conversation_id;
        // Store message in database
        tokio::spawn({
            let content = message.content;
            let role = match message.message_type {
                langchain_rust::schemas::MessageType::HumanMessage => MessageRole::User,
                langchain_rust::schemas::MessageType::AIMessage => MessageRole::Assistant,
                langchain_rust::schemas::MessageType::SystemMessage => MessageRole::System,
                _ => MessageRole::User,
            };

            async move {
                let _ = sqlx::query!(
                    "INSERT INTO conversation_messages (id, conversation_id, role, content, metadata) 
                     VALUES ($1, $2, $3, $4, $5)",
                    Uuid::new_v4(),
                    conversation_id,
                    role.to_string().to_lowercase(),
                    content,
                    serde_json::json!({})
                )
                .execute(&pool)
                .await;
            }
        });
    }

    fn clear(&mut self) {
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let _ = sqlx::query!(
                    "DELETE FROM conversation_messages WHERE conversation_id = $1",
                    self.conversation_id
                )
                .execute(&self.pool)
                .await;
            });
        });
    }
}

pub struct ConversationManager {
    pool: PgPool,
    current_conversation: Option<Uuid>,
}

impl ConversationManager {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            current_conversation: None,
        }
    }

    pub async fn get_most_recent_conversation(&self) -> Result<Option<Conversation>> {
        sqlx::query_as!(
            Conversation,
            "SELECT id, summary, created_at, updated_at, tickers 
             FROM conversations 
             ORDER BY updated_at DESC 
             LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn create_conversation(
        &mut self,
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
        .execute(&self.pool)
        .await?;

        // Add initial system message
        let system_prompt = format!(
            "This is a conversation about the following companies: {}. \
             Focus on providing accurate financial analysis and insights.",
            tickers.join(", ")
        );

        self.add_message(
            &id,
            MessageRole::System,
            system_prompt.to_string(),
            serde_json::json!({}),
        )
        .await?;

        self.current_conversation = Some(id);
        Ok(id)
    }

    pub async fn add_message(
        &self,
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
        .execute(&self.pool)
        .await?;

        Ok(id.to_string())
    }

    pub async fn get_conversation_messages(
        &self,
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
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn update_summary(&mut self, id: &Uuid, summary: String) -> Result<()> {
        sqlx::query!(
            "UPDATE conversations SET summary = $1, updated_at = NOW() WHERE id = $2",
            summary,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_conversation(&self, id: &Uuid) -> Result<Option<Conversation>> {
        sqlx::query_as!(
            Conversation,
            "SELECT id, summary, created_at, updated_at, tickers 
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
            "SELECT id, summary, created_at, updated_at, tickers 
             FROM conversations 
             ORDER BY updated_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn switch_conversation(&mut self, id: &Uuid) -> Result<()> {
        if self.get_conversation(id).await?.is_some() {
            // Update the updated_at timestamp when switching
            sqlx::query!(
                "UPDATE conversations SET updated_at = NOW() WHERE id = $1",
                id
            )
            .execute(&self.pool)
            .await?;

            self.current_conversation = Some(*id);
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

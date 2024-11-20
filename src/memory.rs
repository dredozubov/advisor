use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use langchain_rust::{
    chain::{builder::ConversationalChainBuilder, ConversationalChain},
    llm::{OpenAI, OpenAIConfig},
    schemas::BaseMemory,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{postgres::PgPool, types::Uuid};
use std::{collections::HashMap, sync::Arc};

#[derive(Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
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

pub struct ConversationChainManager {
    pool: PgPool,
    chains: HashMap<String, ConversationalChain>,
}

#[derive(Debug, Clone)]
pub struct ConversationChainManagerRef(Arc<ConversationChainManager>);

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
            let memory = DatabaseMemory::new(
                self.pool.clone(),
                conversation_id,
                10, // window size
            );

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

// Database-backed memory implementation
pub struct DatabaseMemory {
    pool: PgPool,
    conversation_id: Uuid,
    window_size: i64,
}

impl DatabaseMemory {
    pub fn new(pool: PgPool, conversation_id: &Uuid, window_size: i64) -> Self {
        Self {
            pool,
            *conversation_id,
            window_size,
        }
    }

    fn convert_role(role: MessageRole) -> MessageRole {
        match role {
            MessageRole::User => MessageRole::User,
            MessageRole::Assistant => MessageRole::Assistant,
            MessageRole::System => MessageRole::System,
        }
    }

    fn convert_chat_role(role: MessageRole) -> MessageRole {
        match role {
            MessageRole::User => MessageRole::User,
            MessageRole::Assistant => MessageRole::Assistant,
            MessageRole::System => MessageRole::System,
            _ => MessageRole::User, // Default to user for unknown roles
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
        // Store message in database
        tokio::spawn({
            let pool = self.pool.clone();
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
                    self.conversation_id,
                    role.to_string().to_lowercase(),
                    content,
                    serde_json::json!({})
                )
                .execute(&pool)
                .await;
            }
        });
    }

    async fn clear(&mut self) {
        sqlx::query!(
            "DELETE FROM conversation_messages WHERE conversation_id = $1",
            self.conversation_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
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

        self.current_conversation = Some(id.clone());
        Ok(id.clone())
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
                role as "role: MessageRole",
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

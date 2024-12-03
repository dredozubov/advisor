use anyhow::Result;
use async_trait::async_trait;
use core::fmt;
use tokio::sync::RwLock;
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Conversation {
    pub id: Uuid,
    pub user_id: Uuid,
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
    chains: Arc<RwLock<HashMap<String, ConversationalChain>>>,
}

impl ConversationChainManager {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            chains: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_or_create_chain(
        &self,
        conversation_id: &Uuid,
        llm: OpenAI<OpenAIConfig>,
    ) -> Result<Arc<ConversationalChain>> {
        let id_str = conversation_id.to_string();
        let mut chains = self.chains.write().await;
        
        if !chains.contains_key(&id_str) {
            // Create database-backed memory
            let memory = DatabaseMemory::new(self.pool.clone(), *conversation_id);

            // Create new chain with database memory
            let chain = ConversationalChainBuilder::new()
                .llm(llm.clone())
                .memory(Arc::new(tokio::sync::Mutex::new(memory)))
                .build()?;

            chains.insert(id_str.clone(), chain);
        }

        Ok(Arc::new(chains[&id_str].clone()))
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
                let _ =
                    crate::db::clear_conversation_messages(&self.pool, &self.conversation_id).await;
            });
        });
    }
}

#[derive(Clone)]
pub struct ConversationManager {
    pool: PgPool,
    current_conversation: Option<Uuid>,
    user_id: Uuid,
}

impl ConversationManager {
    pub fn new(pool: PgPool, user_id: Uuid) -> Self {
        Self {
            pool,
            current_conversation: None,
            user_id, // Default to CLI user (all zeros)
        }
    }

    pub fn new_cli(pool: PgPool) -> Self {
        Self::new(pool, Uuid::nil())
    }

    pub async fn get_most_recent_conversation(&self) -> Result<Option<Conversation>> {
        crate::db::get_most_recent_conversation(&self.pool, &self.user_id).await
    }

    pub async fn create_conversation(
        &mut self,
        summary: String,
        tickers: Vec<String>,
    ) -> Result<Uuid> {
        let id =
            crate::db::create_conversation(&self.pool, &self.user_id, summary, tickers.clone())
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
        crate::db::add_message(&self.pool, conversation_id, role, content, metadata).await
    }

    pub async fn get_conversation_messages(
        &self,
        conversation_id: &Uuid,
        limit: i64,
    ) -> Result<Vec<Message>> {
        crate::db::get_conversation_messages(&self.pool, conversation_id, limit).await
    }

    pub async fn update_summary(&mut self, id: &Uuid, summary: String) -> Result<()> {
        crate::db::update_conversation_summary(&self.pool, id, summary).await
    }

    pub async fn get_conversation(&self, id: &Uuid) -> Result<Option<Conversation>> {
        crate::db::get_conversation(&self.pool, id, &self.user_id).await
    }

    pub async fn list_conversations(&self) -> Result<Vec<Conversation>> {
        crate::db::list_conversations(&self.pool, &self.user_id).await
    }

    pub async fn switch_conversation(&mut self, id: &Uuid) -> Result<()> {
        if self.get_conversation(id).await?.is_some() {
            crate::db::update_conversation_timestamp(&self.pool, id).await?;
            self.current_conversation = Some(*id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Conversation not found"))
        }
    }

    pub async fn delete_conversation(&mut self, id: &Uuid) -> Result<()> {
        // Verify conversation exists and belongs to user
        if self.get_conversation(id).await?.is_none() {
            return Err(anyhow::anyhow!("Conversation not found"));
        }

        // Delete from database
        sqlx::query!(
            "DELETE FROM conversations WHERE id = $1 AND user_id = $2",
            id,
            self.user_id
        )
        .execute(&self.pool)
        .await?;

        // Clear current conversation if it was the deleted one
        if self.current_conversation == Some(*id) {
            self.current_conversation = None;
        }

        Ok(())
    }

    pub async fn get_current_conversation_details(&self) -> Result<Option<Conversation>> {
        if let Some(id) = &self.current_conversation {
            self.get_conversation(id).await
        } else {
            Ok(None)
        }
    }
}

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPool, types::Uuid};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
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

impl ConversationChainManager {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            chains: HashMap::new(),
        }
    }

    pub async fn get_or_create_chain(
        &mut self,
        conversation_id: &str,
        llm: OpenAI,
    ) -> Result<&ConversationalChain> {
        if !self.chains.contains_key(conversation_id) {
            // Get conversation messages to initialize memory
            let messages = sqlx::query!(
                r#"
                SELECT role as "role: MessageRole", content
                FROM conversation_messages 
                WHERE conversation_id = $1 
                ORDER BY created_at ASC
                "#,
                conversation_id
            )
            .fetch_all(&self.pool)
            .await?;

            // Create memory buffer with existing messages
            let mut memory = WindowBufferMemory::new(10);
            for msg in messages {
                memory.add_message(match msg.role {
                    MessageRole::User => ChatMessage::user(&msg.content),
                    MessageRole::Assistant => ChatMessage::assistant(&msg.content),
                    MessageRole::System => ChatMessage::system(&msg.content),
                });
            }

            // Create new chain with populated memory
            let chain = ConversationalChainBuilder::new()
                .llm(llm.clone())
                .memory(memory.into())
                .build()?;

            self.chains.insert(conversation_id.to_string(), chain);
        }

        Ok(&self.chains[conversation_id])
    }
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

    pub async fn get_most_recent_conversation(&self) -> Result<Option<Conversation>> {
        sqlx::query_as!(
            Conversation,
            "SELECT id, title, summary, created_at, updated_at, tickers 
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
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
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
            &system_prompt,
            serde_json::json!({}),
        ).await?;

        self.current_conversation = Some(id.clone());
        Ok(id)
    }

    pub async fn add_message(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        metadata: Value,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
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

        Ok(id)
    }

    pub async fn get_conversation_messages(
        &self,
        conversation_id: &str,
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
            // Update the updated_at timestamp when switching
            sqlx::query!(
                "UPDATE conversations SET updated_at = NOW() WHERE id = $1",
                id
            )
            .execute(&self.pool)
            .await?;
            
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
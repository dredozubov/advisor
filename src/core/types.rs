use anyhow::Result;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::error::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    pub content: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInfo {
    pub id: Uuid,
    pub summary: String,
    pub tickers: Vec<String>,
}

#[async_trait::async_trait]
pub trait AdvisorBackend {
    async fn process_query(
        &self,
        input: &str,
        conversation_id: &Uuid,
    ) -> Result<BoxStream<'static, Result<String, Box<dyn Error + Send + Sync>>>>;

    async fn create_conversation(&self, summary: String, tickers: Vec<String>) -> Result<Uuid>;
    
    async fn get_conversation(&self, id: &Uuid) -> Result<Option<ConversationInfo>>;
    
    async fn list_conversations(&self) -> Result<Vec<ConversationInfo>>;
    
    async fn update_conversation_summary(&self, id: &Uuid, summary: String) -> Result<()>;
}

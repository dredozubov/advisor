use super::types::{AdvisorBackend, ConversationInfo};
use crate::{eval, memory::ConversationManager};
use anyhow::Result;
use futures::stream::BoxStream;
use langchain_rust::{chain::ConversationalChain, vectorstore::pgvector::Store};
use reqwest::Client;
use std::{error::Error, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct AdvisorService {
    conversation_manager: Arc<RwLock<ConversationManager>>,
    store: Arc<Store>,
    http_client: Client,
    stream_chain: ConversationalChain,
    query_chain: ConversationalChain,
}

impl AdvisorService {
    pub fn new(
        conversation_manager: ConversationManager,
        store: Arc<Store>,
        http_client: Client,
        stream_chain: ConversationalChain,
        query_chain: ConversationalChain,
    ) -> Self {
        Self {
            conversation_manager: Arc::new(RwLock::new(conversation_manager)),
            store,
            http_client,
            stream_chain,
            query_chain,
        }
    }
}

#[async_trait::async_trait]
impl AdvisorBackend for AdvisorService {
    async fn process_query(
        &self,
        input: &str,
        conversation_id: &Uuid,
    ) -> Result<BoxStream<'static, Result<String, Box<dyn Error + Send + Sync>>>> {
        let conversation = self
            .conversation_manager
            .read()
            .await
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Conversation not found"))?;

        let (stream, summary) = eval::eval(
            input,
            &conversation,
            &self.http_client,
            &self.stream_chain,
            &self.query_chain,
            Arc::clone(&self.store),
            self.conversation_manager.clone(),
            self.stream_chain.llm().clone(),
        )
        .await?;

        self.conversation_manager
            .write()
            .await
            .update_summary(conversation_id, summary)
            .await?;

        Ok(stream)
    }

    async fn create_conversation(&self, summary: String, tickers: Vec<String>) -> Result<Uuid> {
        self.conversation_manager
            .write()
            .await
            .create_conversation(summary, tickers)
            .await
    }

    async fn get_conversation(&self, id: &Uuid) -> Result<Option<ConversationInfo>> {
        let conv = self
            .conversation_manager
            .read()
            .await
            .get_conversation(id)
            .await?;
        Ok(conv.map(|c| ConversationInfo {
            id: c.id,
            summary: c.summary,
            tickers: c.tickers,
        }))
    }

    async fn list_conversations(&self) -> Result<Vec<ConversationInfo>> {
        let convs = self
            .conversation_manager
            .read()
            .await
            .list_conversations()
            .await?;
        Ok(convs
            .into_iter()
            .map(|c| ConversationInfo {
                id: c.id,
                summary: c.summary,
                tickers: c.tickers,
            })
            .collect())
    }

    async fn update_conversation_summary(&self, id: &Uuid, summary: String) -> Result<()> {
        self.conversation_manager
            .write()
            .await
            .update_summary(id, summary)
            .await
    }
}

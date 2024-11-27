use crate::core::config::AdvisorConfig;
use anyhow::{Error, Result};
use langchain_rust::{
    chain::{builder::ConversationalChainBuilder, ConversationalChain},
    embedding::openai::OpenAiEmbedder,
    llm::{
        openai::{OpenAI, OpenAIModel},
        OpenAIConfig,
    },
    memory::WindowBufferMemory,
    vectorstore::pgvector::{Store, StoreBuilder},
};
use std::sync::Arc;

use crate::db;

pub async fn initialize_openai(config: &AdvisorConfig) -> Result<OpenAI<OpenAIConfig>> {
    let llm = OpenAI::default()
        .with_config(OpenAIConfig::default().with_api_key(config.openai_key.clone()))
        .with_model(OpenAIModel::Gpt4oMini.to_string());

    Ok(llm)
}

pub async fn initialize_vector_store(config: &AdvisorConfig) -> Result<Arc<Store>> {
    let embedder = OpenAiEmbedder::default()
        .with_config(OpenAIConfig::default().with_api_key(config.openai_key.clone()));

    let store = StoreBuilder::new()
        .embedder(embedder)
        .connection_url(&config.database_url)
        .collection_table_name(db::COLLECTIONS_TABLE)
        .embedder_table_name(db::EMBEDDER_TABLE)
        .vector_dimensions(1536)
        .build()
        .await
        .map_err(|e| Error::msg(e.to_string()))?;

    Ok(Arc::new(store))
}

pub async fn initialize_chains(
    llm: OpenAI<OpenAIConfig>,
) -> Result<(ConversationalChain, ConversationalChain)> {
    let stream_memory = WindowBufferMemory::new(10);
    let query_memory = WindowBufferMemory::new(10);

    let stream_chain = ConversationalChainBuilder::new()
        .llm(llm.clone())
        .memory(stream_memory.into())
        .build()?;

    let query_chain = ConversationalChainBuilder::new()
        .llm(llm)
        .memory(query_memory.into())
        .build()?;

    Ok((stream_chain, query_chain))
}

use anyhow::Error;
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

pub async fn initialize_openai() -> Result<(OpenAI<OpenAIConfig>, String), Box<dyn Error>> {
    let openai_key = std::env::var("OPENAI_KEY").map_err(|_| -> Box<dyn Error> {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "OPENAI_KEY environment variable not set. Please run with: OPENAI_KEY=your-key-here cargo run"
        ))
    })?;

    let llm = OpenAI::default()
        .with_config(OpenAIConfig::default().with_api_key(openai_key.clone()))
        .with_model(OpenAIModel::Gpt4oMini.to_string());

    Ok((llm, openai_key))
}

pub async fn initialize_vector_store(
    openai_key: String,
    pg_connection_string: String,
) -> Result<Arc<Store>, Box<dyn Error>> {
    let embedder = OpenAiEmbedder::default()
        .with_config(OpenAIConfig::default().with_api_key(openai_key));

    let store = StoreBuilder::new()
        .embedder(embedder)
        .connection_url(&pg_connection_string[..])
        .collection_table_name(db::COLLECTIONS_TABLE)
        .embedder_table_name(db::EMBEDDER_TABLE)
        .vector_dimensions(1536)
        .build()
        .await?;

    Ok(Arc::new(store))
}

pub async fn initialize_chains(
    llm: OpenAI<OpenAIConfig>,
) -> Result<(ConversationalChain, ConversationalChain), Box<dyn Error>> {
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

use advisor::{
    db,
    edgar::filing,
    eval,
    memory::{ConversationChainManager, ConversationManager},
    repl::{self, EditorWithHistory},
    utils::dirs,
};
use colored::*;
use futures::StreamExt;
use langchain_rust::chain::builder::ConversationalChainBuilder;
use langchain_rust::llm::openai::{OpenAI, OpenAIModel};
use langchain_rust::llm::OpenAIConfig;
use langchain_rust::memory::WindowBufferMemory;
use langchain_rust::vectorstore::pgvector::StoreBuilder;
use langchain_rust::{chain::ConversationalChain, vectorstore::VectorStore};
use rustyline::error::ReadlineError;
use std::{
    env, fs,
    str::FromStr,
    sync::{Arc, RwLock},
};
use std::{error::Error, io::Write};
use uuid::Uuid;

async fn initialize_openai() -> Result<(OpenAI<OpenAIConfig>, String), Box<dyn Error>> {
    let openai_key = env::var("OPENAI_KEY").map_err(|_| -> Box<dyn Error> {
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

async fn initialize_vector_store(
    openai_key: String,
) -> Result<Box<dyn VectorStore>, Box<dyn Error>> {
    let embedder = langchain_rust::embedding::openai::OpenAiEmbedder::default()
        .with_config(OpenAIConfig::default().with_api_key(openai_key));

    let pg_connection_string = "postgres://postgres:password@localhost:5432/advisor";

    let store = StoreBuilder::new()
        .embedder(embedder)
        .connection_url(pg_connection_string)
        .collection_table_name(db::COLLECTIONS_TABLE)
        .embedder_table_name(db::EMBEDDER_TABLE)
        .vector_dimensions(1536)
        .build()
        .await?;

    Ok(Box::new(store))
}

async fn initialize_chains(
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

async fn handle_command(
    cmd: &str,
    rl: &mut EditorWithHistory,
    conversation_manager: &Arc<RwLock<ConversationManager>>,
    chain_manager: &mut ConversationChainManager,
    llm: OpenAI<OpenAIConfig>,
) -> Result<(), Box<dyn Error>> {
    match cmd {
        "/new" => {
            let tickers = rl.readline("Enter tickers (comma-separated): ")?;
            let tickers: Vec<String> = tickers
                .split(',')
                .map(|s| s.trim().to_uppercase())
                .collect();
            let summary = format!("New conversation about: {}", tickers.join(", "));
            let conv_id = conversation_manager
                .write()
                .unwrap()
                .create_conversation(summary, tickers)
                .await?;

            chain_manager.get_or_create_chain(&conv_id, llm).await?;
        }
        "/list" => {
            let conversations = conversation_manager
                .read()
                .unwrap()
                .list_conversations()
                .await?;
            for conv in conversations {
                println!(
                    "{}: {} [{}]\n",
                    conv.id,
                    conv.summary.blue().bold(),
                    conv.tickers.join(", ").yellow()
                );
            }
        }
        "/switch" => {
            let id = rl.readline("Enter conversation ID: ")?;
            let uuid = Uuid::from_str(&id)?;
            conversation_manager
                .write()
                .unwrap()
                .switch_conversation(&uuid)
                .await?;
        }
        _ => {}
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    env_logger::init();
    log::debug!("Logger initialized");

    let (llm, openai_key) = initialize_openai().await?;

    let store = initialize_vector_store(openai_key).await?;

    let pg_connection_string = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pg_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .connect(&pg_connection_string[..])
        .await?;

    log::debug!("Creating data directory at {}", dirs::EDGAR_FILINGS_DIR);
    fs::create_dir_all(dirs::EDGAR_FILINGS_DIR)?;

    let (stream_chain, query_chain) = initialize_chains(llm.clone()).await?;

    let mut rl = repl::create_editor().await?;

    let http_client = reqwest::Client::builder()
        .user_agent(filing::USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    println!("Enter 'quit' to exit");
    let mut conversation_manager = ConversationManager::new(pg_pool.clone());
    let mut chain_manager = ConversationChainManager::new(pg_pool);

    if let Some(recent_conv) = conversation_manager.get_most_recent_conversation().await? {
        conversation_manager
            .switch_conversation(&recent_conv.id)
            .await?;
        println!(
            "Loaded most recent conversation: {}",
            recent_conv.summary.blue().bold()
        );
    }

    let conversation_manager = Arc::new(conversation_manager);
    let conversation_manager = Arc::new(RwLock::new(Arc::clone(&conversation_manager)));

    loop {
        let current_conv = conversation_manager
            .read()
            .unwrap()
            .get_current_conversation_details()
            .await?;
        let prompt = get_prompt(&current_conv.map(|c| c.summary.clone()).unwrap_or_default());

        match rl.readline(&prompt) {
            Ok(line) => {
                let input = line.trim();
                if input == "quit" {
                    break;
                }

                if input.starts_with('/') {
                    handle_command(
                        input,
                        &mut rl,
                        &conversation_manager,
                        &mut chain_manager,
                        llm.clone(),
                    )
                    .await?;
                    continue;
                }

                if let Some(conv) = current_conv {
                    match eval::eval(
                        input,
                        &conv,
                        &http_client,
                        &stream_chain,
                        &query_chain,
                        store.as_ref(),
                        &pg_pool,
                        Arc::clone(&conversation_manager),
                    )
                    .await
                    {
                        Ok((mut stream, new_summary)) => {
                            conversation_manager
                                .write()
                                .unwrap()
                                .update_summary(&conv.id, new_summary)
                                .await?;

                            while let Some(chunk) = stream.next().await {
                                match chunk {
                                    Ok(c) => {
                                        print!("{}", c);
                                        std::io::stdout().flush()?;
                                    }
                                    Err(e) => {
                                        eprintln!("\nStream error: {}", e);
                                        break;
                                    }
                                }
                            }
                            println!();
                        }
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    println!("Goodbye!");

    // Save history
    log::debug!("Saving REPL history");
    repl::save_history(&mut rl)?;
    log::debug!("REPL history saved successfully");

    Ok(())
}

fn get_prompt(summary: &str) -> String {
    if summary.is_empty() {
        format!("{}", "> ".green().bold())
    } else {
        format!("{} {}", summary.blue().bold(), " > ".green().bold())
    }
}

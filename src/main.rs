use advisor::{
    core::init,
    db,
    edgar::filing,
    eval,
    memory::{ConversationChainManager, ConversationManager},
    repl::{self, EditorWithHistory},
    utils::dirs,
};
use colored::*;
use futures::StreamExt;
use langchain_rust::chain::ConversationalChain;
use langchain_rust::llm::openai::{OpenAI, OpenAIModel};
use langchain_rust::llm::OpenAIConfig;
use langchain_rust::memory::WindowBufferMemory;
use langchain_rust::vectorstore::pgvector::StoreBuilder;
use langchain_rust::{chain::builder::ConversationalChainBuilder, vectorstore::pgvector::Store};
use rustyline::error::ReadlineError;
use std::{env, fs, str::FromStr, sync::Arc};
use std::{error::Error, io::Write};
use tokio::sync::RwLock;
use uuid::Uuid;


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
                .await
                .create_conversation(summary, tickers)
                .await?;

            chain_manager.get_or_create_chain(&conv_id, llm).await?;
        }
        "/list" => {
            let conversations = conversation_manager
                .read()
                .await
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
                .await
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

    let (llm, openai_key) = init::initialize_openai().await?;

    let pg_connection_string = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let store = init::initialize_vector_store(openai_key, pg_connection_string.clone()).await?;

    let pg_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .connect(&pg_connection_string[..])
        .await?;

    log::debug!("Creating data directory at {}", dirs::EDGAR_FILINGS_DIR);
    fs::create_dir_all(dirs::EDGAR_FILINGS_DIR)?;

    let (stream_chain, query_chain) = init::initialize_chains(llm.clone()).await?;

    let mut rl = repl::create_editor().await?;

    let http_client = reqwest::Client::builder()
        .user_agent(filing::USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    println!("Enter 'quit' to exit");
    let mut conversation_manager = ConversationManager::new(pg_pool.clone());
    let mut chain_manager = ConversationChainManager::new(pg_pool.clone());

    if let Some(recent_conv) = conversation_manager.get_most_recent_conversation().await? {
        conversation_manager
            .switch_conversation(&recent_conv.id)
            .await?;
        println!(
            "Loaded most recent conversation: {}",
            recent_conv.summary.blue().bold()
        );
    }

    // Keep one Arc<ConversationManager> for eval
    let conversation_manager_for_eval = Arc::new(conversation_manager.clone());
    // And one Arc<RwLock<ConversationManager>> for thread-safe access
    let conversation_manager = Arc::new(RwLock::new(conversation_manager));

    loop {
        let current_conv = conversation_manager
            .read()
            .await
            .get_current_conversation_details()
            .await?;
        let prompt = get_prompt(
            &current_conv
                .clone()
                .map(|c| c.summary.clone())
                .unwrap_or_default(),
        );

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
                        Arc::clone(&store),
                        conversation_manager_for_eval.clone(),
                    )
                    .await
                    {
                        Ok((mut stream, new_summary)) => {
                            conversation_manager
                                .write()
                                .await
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

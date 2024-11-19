use advisor::{db, edgar::filing, eval, memory::ConversationManager, repl, utils::dirs};
use futures::StreamExt;
use langchain_rust::chain::builder::ConversationalChainBuilder;
use langchain_rust::llm::openai::{OpenAI, OpenAIModel};
use langchain_rust::llm::OpenAIConfig;
use langchain_rust::memory::WindowBufferMemory;
use langchain_rust::vectorstore::pgvector::StoreBuilder;
use rustyline::error::ReadlineError;
use std::{env, fs};
use std::{error::Error, io::Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize the logger.
    env_logger::init();
    log::debug!("Logger initialized");

    let openai_key = match env::var("OPENAI_KEY") {
        Ok(key) => key,
        Err(_) => {
            eprintln!("OPENAI_KEY environment variable not set");
            eprintln!("Please run the program with:");
            eprintln!("OPENAI_KEY=your-key-here cargo run");
            std::process::exit(1);
        }
    };

    // Initialize OpenAI embedder
    let embedder = langchain_rust::embedding::openai::OpenAiEmbedder::default()
        .with_config(OpenAIConfig::default().with_api_key(openai_key.clone()));

    // Create separate memory buffers for each chain
    let stream_memory = WindowBufferMemory::new(10);
    let query_memory = WindowBufferMemory::new(10);

    let pg_connection_string = "postgres://postgres:postgres@localhost:5432/advisor";

    let pg_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .connect(pg_connection_string)
        .await?;

    let store = StoreBuilder::new()
        .embedder(embedder)
        .connection_url(pg_connection_string)
        .collection_table_name(db::COLLECTIONS_TABLE)
        .embedder_table_name(db::EMBEDDER_TABLE)
        .vector_dimensions(1536)
        .build()
        .await?;

    log::debug!("Creating data directory at {}", dirs::EDGAR_FILINGS_DIR);
    fs::create_dir_all(dirs::EDGAR_FILINGS_DIR)?;
    log::debug!("Data directory created successfully");

    // Initialize ChatGPT executor with API key from environment
    log::debug!("Initializing OpenAI client");

    let llm = OpenAI::default()
        .with_config(OpenAIConfig::default().with_api_key(openai_key))
        .with_model(OpenAIModel::Gpt4oMini.to_string());
    log::debug!("OpenAI client initialized successfully");

    // Create two separate chains - one for streaming responses and one for query generation
    // TODO: create the chain within at the invoke point.
    let stream_chain = ConversationalChainBuilder::new()
        .llm(llm.clone())
        .memory(stream_memory.into())
        .build()
        .expect("Error building streaming ConversationalChain");

    let query_chain = ConversationalChainBuilder::new()
        .llm(llm)
        .memory(query_memory.into())
        .build()
        .expect("Error building query ConversationalChain");

    // Create a rustyline Editor
    log::debug!("Creating rustyline editor");
    let mut rl = repl::create_editor().await?;
    log::debug!("Rustyline editor created successfully");

    // You can also include timeouts and other settings
    log::debug!("Building HTTP client");
    let http_client = reqwest::Client::builder()
        .user_agent(filing::USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to create client");
    log::debug!("HTTP client built successfully");

    log::debug!("Starting REPL loop");
    println!("Enter 'quit' to exit");
    let mut conversation_manager = ConversationManager::new(pg_pool.clone());
    let mut chain_manager = ConversationChainManager::new(pg_pool.clone());

    // Load most recent conversation on startup
    if let Some(recent_conv) = conversation_manager.get_most_recent_conversation().await? {
        conversation_manager.switch_conversation(recent_conv.id).await?;
        println!(
            "Loaded most recent conversation: {} ({})",
            recent_conv.title.blue().bold(),
            recent_conv.summary.yellow()
        );
    }
    
    loop {
        let current_conv = conversation_manager.get_current_conversation_details().await?;
        
        let prompt = match &current_conv {
            Some(conv) => {
                format!(
                    "[{}] {} {}",
                    conv.title.blue().bold(),
                    conv.summary.yellow(),
                    ">".green().bold()
                )
            }
            None => format!("{}", "[No conversation] >".green().bold()),
        };
        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                let input = line.trim();
                match input {
                    "/new" => {
                        let title = rl.readline("Enter conversation title: ")?;
                        let tickers = rl.readline("Enter tickers (comma-separated): ")?;
                        let tickers: Vec<String> = tickers
                            .split(',')
                            .map(|s| s.trim().to_uppercase())
                            .collect();
                        let conv_id = conversation_manager
                            .create_conversation(title, tickers)
                            .await?;
            
                        // Initialize chain for new conversation
                        chain_manager
                            .get_or_create_chain(&conv_id, llm.clone())
                            .await?;
                    }
                    "/list" => {
                        let conversations = conversation_manager.list_conversations().await?;
                        for conv in conversations {
                            println!(
                                "{}: {} [{}]\n  Context: {}\n",
                                conv.id,
                                conv.title.blue().bold(),
                                conv.tickers.join(", ").yellow(),
                                conv.summary
                            );
                        }
                    }
                    "/switch" => {
                        let id = rl.readline("Enter conversation ID: ")?;
                        conversation_manager.switch_conversation(id).await?;
                    }
                    "quit" => break,
                    _ => {
                        if let Some(conv) = current_conv {
                            // Get or create chain for current conversation
                            let chain = chain_manager
                                .get_or_create_chain(&conv.id, llm.clone())
                                .await?;

                            match eval::eval(
                                input,
                                &conv,
                                &http_client,
                                chain,
                                chain,
                                &store,
                                &pg_pool,
                                &conversation_manager,
                            )
                            .await
                {
                    Ok((mut stream, new_summary)) => {
                        conversation_manager
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
                        println!(); // Add newline after stream ends
                    }
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
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

use colored::*;

fn get_prompt(summary: &str) -> String {
    if summary.is_empty() {
        format!("{}", "> ".green().bold())
    } else {
        format!("{} {}", summary.blue().bold(), " > ".green().bold())
    }
}

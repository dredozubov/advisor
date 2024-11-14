use advisor::{edgar::filing, eval, repl, utils::dirs};
use futures::StreamExt;
use langchain_rust::chain::builder::ConversationalChainBuilder;
use langchain_rust::llm::openai::{OpenAI, OpenAIModel};
use langchain_rust::llm::OpenAIConfig;
use langchain_rust::memory::WindowBufferMemory;
use langchain_rust::vectorstore::qdrant::{Qdrant, StoreBuilder};
use qdrant_client::config::QdrantConfig;
use rustyline::error::ReadlineError;
use std::{env, fs};
use std::{error::Error, io::Write};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "advisor", about = "AI-powered financial document analysis")]
struct Opt {
    /// Qdrant server URI (only used if storage=qdrant)
    #[structopt(long, default_value = "http://localhost:6334")]
    qdrant_uri: String,

    /// Qdrant collection name (only used if storage=qdrant)
    #[structopt(long, default_value = "advisor")]
    qdrant_collection: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize the logger.
    env_logger::init();
    log::debug!("Logger initialized");

    let opt = Opt::from_args();
    let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY environment variable must be set");

    // Initialize OpenAI embedder
    let embedder = langchain_rust::embedding::openai::OpenAiEmbedder::default()
        .with_config(OpenAIConfig::default().with_api_key(openai_key.clone()));

    let qdrant_client = Qdrant::new(QdrantConfig::from_url(&opt.qdrant_uri[..]))?;

    // Only keep last 10 messages in memory
    let memory = WindowBufferMemory::new(10);

    let store = StoreBuilder::new()
        .embedder(embedder)
        .client(qdrant_client)
        .collection_name(&opt.qdrant_collection)
        .build()
        .await
        .unwrap();

    log::debug!("Creating data directory at {}", dirs::EDGAR_FILINGS_DIR);
    fs::create_dir_all(dirs::EDGAR_FILINGS_DIR)?;
    log::debug!("Data directory created successfully");

    // Initialize ChatGPT executor with API key from environment
    log::debug!("Initializing OpenAI client");

    let llm = OpenAI::default()
        .with_config(OpenAIConfig::default().with_api_key(openai_key))
        .with_model(OpenAIModel::Gpt4oMini.to_string());
    log::debug!("OpenAI client initialized successfully");

    // Create the conversational chain
    let chain = ConversationalChainBuilder::new()
        .llm(llm.clone())
        .memory(memory.into())
        .build()
        .expect("Error building ConversationalChain");

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
    let mut summary = String::new();
    loop {
        let prompt = get_prompt(&summary[..]);
        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                let input = line.trim();
                if input.eq_ignore_ascii_case("quit") {
                    break;
                }

                // Process the input using the eval function
                match eval::eval(input, &http_client, &chain, &store).await {
                    Ok((mut stream, new_summary)) => {
                        summary = new_summary;
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

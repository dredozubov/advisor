use advisor::{
    core::{config::AdvisorConfig, init},
    edgar::filing,
    eval,
    memory::{ConversationChainManager, ConversationManager},
    repl::{self, EditorWithHistory},
    utils::dirs,
};
use colored::*;
use crossterm::execute;
use futures::StreamExt;
use langchain_rust::llm::openai::{OpenAI, OpenAIConfig};
use rustyline::error::ReadlineError;
use std::{
    error::Error,
    io::{stdout, Write},
    sync::atomic::{AtomicBool, Ordering},
};
use std::{fs, str::FromStr, sync::Arc};
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
            let conv_id = conversation_manager
                .write()
                .await
                .create_conversation("New conversation".to_string(), vec![])
                .await?;

            chain_manager.get_or_create_chain(&conv_id, llm).await?;
            println!("Started new conversation. Please enter your first question with at least one valid ticker symbol (e.g. @AAPL)");
        }
        "/list" => {
            let mut cm = conversation_manager.write().await;
            match repl::handle_list_command(&mut cm, rl).await {
                Ok(msg) => println!("{}", msg),
                Err(e) => eprintln!("Error listing conversations: {}", e),
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

fn get_prompt(summary: &str) -> String {
    if summary.is_empty() {
        format!("{}", "> ".green().bold())
    } else {
        format!("{} {}", summary.blue().bold(), " > ".green().bold())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Set up signal handlers for cleanup
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C!");
        r.store(false, Ordering::SeqCst);
    })?;
    dotenv::dotenv().ok();
    env_logger::init();
    log::debug!("Logger initialized");

    let config = AdvisorConfig::from_env()?;

    let llm = init::initialize_openai(&config).await?;
    let store = init::initialize_vector_store(&config).await?;

    let pg_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .connect(&config.database_url)
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
    let mut conversation_manager = ConversationManager::new_cli(pg_pool.clone());
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

    // Create thread-safe conversation manager
    let conversation_manager = Arc::new(RwLock::new(conversation_manager));
    // Clone it for eval
    let conversation_manager_for_eval = Arc::clone(&conversation_manager);

    while running.load(Ordering::SeqCst) {
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

        match rl.readline_with_initial(&prompt, ("", "")) {
            Ok(line) => {
                if *line.as_bytes() == [20] {
                    // Ctrl+T ASCII code
                    let conv_id = conversation_manager
                        .write()
                        .await
                        .create_conversation("New conversation".to_string(), vec![])
                        .await?;
                    conversation_manager
                        .write()
                        .await
                        .switch_conversation(&conv_id)
                        .await?;
                    println!("Started new conversation. Please enter your first question with at least one valid ticker symbol (e.g. @AAPL)");
                    continue;
                }

                let input = line.trim();
                if input == "quit" {
                    // Ensure terminal is back to normal mode before quitting
                    if crossterm::terminal::is_raw_mode_enabled()? {
                        crossterm::terminal::disable_raw_mode()?;
                    }
                    execute!(
                        stdout(),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        crossterm::cursor::Show,
                        crossterm::terminal::LeaveAlternateScreen
                    )?;
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

    // Cleanup
    println!("\nGoodbye!");

    // Save history
    log::debug!("Saving REPL history");
    repl::save_history(&mut rl)?;
    log::debug!("REPL history saved successfully");

    // Ensure terminal is back to normal mode
    if crossterm::terminal::is_raw_mode_enabled()? {
        crossterm::terminal::disable_raw_mode()?;
    }

    execute!(
        stdout(),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::Show,
        crossterm::terminal::LeaveAlternateScreen
    )?;

    Ok(())
}

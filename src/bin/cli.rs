use advisor::{
    core::{config::AdvisorConfig, init},
    edgar::filing,
    eval,
    memory::{ConversationChainManager, ConversationManager, MessageRole},
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
        "/history" => {
            if let Some(conv) = conversation_manager
                .read()
                .await
                .get_current_conversation_details()
                .await?
            {
                let messages = conversation_manager
                    .read()
                    .await
                    .get_conversation_messages(&conv.id, 100)
                    .await?;

                println!("\nConversation history:");
                for msg in messages.iter().rev() {
                    let role_color = match msg.role {
                        MessageRole::User => "cyan",
                        MessageRole::Assistant => "green",
                        MessageRole::System => "yellow",
                    };

                    // Take first 100 words
                    let content_preview: String = msg
                        .content
                        .split_whitespace()
                        .take(100)
                        .collect::<Vec<_>>()
                        .join(" ");

                    // Add ellipsis if content was truncated
                    let display_content = if msg.content.split_whitespace().count() > 100 {
                        format!("{}...", content_preview)
                    } else {
                        content_preview
                    };

                    println!(
                        "\n{}: {}",
                        msg.role.to_string().color(role_color),
                        display_content
                    );

                    // Display document chunks used for this message
                    if msg.role == MessageRole::Assistant {
                        // Extract chunks from message metadata
                        if let Some(query) = msg.metadata.get("query") {
                            if let Some(chunks) = query.get("chunks").and_then(|c| c.as_array()) {
                                let chunk_info = chunks.iter()
                                    .filter_map(|c| c.as_str())
                                    .map(|chunk| {
                                        let mut parts = chunk.split(':');
                                        match (parts.next(), parts.next()) {
                                            (Some("filing"), Some(rest)) => format!("Filing {}", rest),
                                            (Some("earnings"), Some(rest)) => format!("Earnings {}", rest),
                                            _ => chunk.to_string()
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                if !chunk_info.is_empty() {
                                    println!("  {}", format!("Referenced documents: {}", chunk_info).dimmed());
                                }
                            }
                        }
                    }
                }
                println!(); // Extra newline for spacing
            } else {
                println!("No active conversation.");
            }
        }
        "/new" => {
            start_new_conversation(conversation_manager, chain_manager, llm).await?;
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

async fn start_new_conversation(
    conversation_manager: &Arc<RwLock<ConversationManager>>,
    chain_manager: &mut ConversationChainManager,
    llm: OpenAI<OpenAIConfig>,
) -> Result<(), Box<dyn Error>> {
    let conv_id = conversation_manager
        .write()
        .await
        .create_conversation("New conversation".to_string(), vec![])
        .await?;
    chain_manager.get_or_create_chain(&conv_id, llm).await?;
    println!("Started new conversation. Please enter your first question with at least one valid ticker symbol (e.g. @AAPL)");
    Ok(())
}

fn get_prompt(summary: &str, token_usage: &advisor::TokenUsage) -> String {
    if summary.is_empty() {
        format!("{}", "> ".green().bold())
    } else {
        let formatted = token_usage.format_prompt(summary);
        format!("{} {}", formatted.blue().bold(), " > ".green().bold())
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

    println!("Enter 'quit' to exit");
    let token_usage = Arc::new(advisor::TokenUsage::default());
    let mut conversation_manager = ConversationManager::new_cli(pg_pool.clone());
    let mut chain_manager = ConversationChainManager::new(pg_pool.clone());

    let mut rl = repl::create_editor(
        conversation_manager.clone(),
        Arc::new(chain_manager.clone()),
        llm.clone(),
    )
    .await?;

    let http_client = reqwest::Client::builder()
        .user_agent(filing::USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    let recent_conv = conversation_manager.get_most_recent_conversation().await?;
    match recent_conv {
        Some(conv) => {
            conversation_manager.switch_conversation(&conv.id).await?;
            println!(
                "{} {}",
                "Loaded most recent conversation:".green(),
                conv.summary.blue().bold()
            );
        }
        None => {
            println!(
                "{}",
                "No previous conversations found. Starting fresh!".yellow()
            );
        }
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
        let summary = current_conv
            .clone()
            .map(|c| c.summary.clone())
            .unwrap_or_default();

        // Update token count for current conversation
        if let Some(conv) = &current_conv {
            let messages = conversation_manager
                .read()
                .await
                .get_conversation_messages(&conv.id, 100)
                .await?;
            let full_text = messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            token_usage.update_current_tokens(&full_text);
        }

        let prompt = get_prompt(&summary, &token_usage);

        match rl.readline_with_initial(&prompt, ("", "")) {
            Ok(line) => {
                // Add to history before any processing
                let input = line.trim().to_string();
                if !input.is_empty() {
                    rl.add_history_entry(&input)?;
                }

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
                        &input,
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
                        &input,
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

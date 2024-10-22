use anthropic::client::Client;
use anthropic::config::AnthropicConfig;
use chrono::NaiveDate;
use claude_api_interaction::edgar::index::{update_full_index_feed, Config};
use claude_api_interaction::edgar::tickers::fetch_tickers;
use claude_api_interaction::repl;
use rustyline::error::ReadlineError;
use rustyline::{CompletionType, Config as RustylineConfig, EditMode, Editor};
use std::error::Error;
use std::path::PathBuf;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize the logger.
    env_logger::init();

    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");
    println!("API Key (first 5 chars): {}", &api_key[..5]);

    // Create a Config instance
    let config = Config {
        index_start_date: NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
        index_end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
        full_index_data_dir: PathBuf::from("edgar_data/"),
        edgar_full_master_url: Url::parse(
            "https://www.sec.gov/Archives/edgar/full-index/master.idx",
        )?,
        edgar_archives_url: Url::parse("https://www.sec.gov/Archives/")?,
        index_files: vec![
            "master.idx".to_string(),
            "form.idx".to_string(),
            "company.idx".to_string(),
        ],
        user_agent: "Example@example.com".to_string(),
    };

    let tickers = fetch_tickers().await?;
    println!("Fetched {} tickers", tickers.len());

    // Call update_full_index_feed
    println!("Updating full index feed...");
    update_full_index_feed(&config).await?;
    println!("Full index feed updated successfully.");

    // Build from configuration.
    let cfg = AnthropicConfig::new()?;
    let _client = Client::try_from(cfg)?;

    // Create a rustyline Editor with custom configuration
    let rustyline_config = RustylineConfig::builder()
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .build();

    let mut rl = Editor::with_config(rustyline_config)?;

    // Add helper
    let helper = repl::ReplHelper::new();
    rl.set_helper(Some(helper));

    println!("Enter 'quit' to exit");
    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let input = line.trim();
                if input.eq_ignore_ascii_case("quit") {
                    break;
                }

                // Add the input to history
                rl.add_history_entry(input);

                // Process the input (you can add your logic here)
                println!("You entered: {}", input);
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
    Ok(())
}

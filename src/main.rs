use anthropic::client::Client;
use anthropic::config::AnthropicConfig;
use chrono::NaiveDate;
use claude_api_interaction::edgar::index::Config;
use claude_api_interaction::repl;
use claude_api_interaction::eval;
use rustyline::error::ReadlineError;
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

    // Build from configuration.
    let cfg = AnthropicConfig::new()?;
    let _client = Client::try_from(cfg)?;

    // Create a rustyline Editor
    let mut rl = repl::create_editor()?;

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

                // Process the input using the eval function
                match eval::eval(input, &config).await {
                    Ok(result) => println!("{}", result),
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
    repl::save_history(&mut rl)?;
    
    Ok(())
}

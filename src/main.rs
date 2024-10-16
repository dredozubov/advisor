use std::error::Error;
use std::io::{self, Write};

use anthropic::client::Client;
use anthropic::config::AnthropicConfig;
use anthropic::types::Role;

mod edgar;

use chrono::NaiveDate;
use edgar::index::{update_full_index_feed, Config};
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

    // Call update_full_index_feed
    println!("Updating full index feed...");
    update_full_index_feed(&config).await?;
    println!("Full index feed updated successfully.");

    // Build from configuration.
    let cfg = AnthropicConfig::new()?;
    let _client = Client::try_from(cfg)?;

    loop {
        print!("'quit' to exit\n> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let input = input.trim();

        if input.to_lowercase() == "quit" {
            break;
        }

        // match edgar::index::get_latest_10q(input) {
        //     Ok(content) => {
        //         println!(
        //             "Retrieved 10-Q content for {}. First 200 characters:",
        //             input
        //         );
        //         // println!("{:?}", &content);

        //         let messages = vec![Message {
        //             role: Role::User,
        //             content: vec![ContentBlock::Text {
        //                 text: format!("Summarize this 10-Q report: {:?}", content),
        //             }],
        //         }];

        //         let messages_request = MessagesRequestBuilder::default()
        //             .messages(messages.clone())
        //             .model("claude-3-sonnet-20240229".to_string())
        //             .max_tokens(1000usize)
        //             .build()?;

        //         // Send a completion request.
        //         let messages_response = client.messages(messages_request).await?;

        //         // Extract and print the assistant's response
        //         if let Some(content) = messages_response.content.first() {
        //             if let ContentBlock::Text { text } = content {
        //                 println!("Claude's summary: {}", text);
        //             }
        //         }
        //     }
        //     Err(e) => println!("Error retrieving 10-Q: {}", e),
        // }

        println!(); // Add a blank line for readability
    }

    println!("Goodbye!");
    Ok(())
}

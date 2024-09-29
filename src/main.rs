use std::error::Error;
use std::io::{self, Write};

use anthropic::client::Client;
use anthropic::config::AnthropicConfig;
use anthropic::types::{ContentBlock, Message, MessagesRequestBuilder, Role};

mod edgar;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize the logger.
    env_logger::init();

    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");
    println!("API Key (first 5 chars): {}", &api_key[..5]);

    // Build from configuration.
    let cfg = AnthropicConfig::new()?;
    let client = Client::try_from(cfg)?;

    loop {
        print!("Enter a ticker symbol (or 'quit' to exit): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let input = input.trim();

        if input.to_lowercase() == "quit" {
            break;
        }

        match edgar::get_latest_10q(input).await {
            Ok(content) => {
                println!("Retrieved 10-Q content for {}. First 200 characters:", input);
                println!("{}", &content[..200.min(content.len())]);

                let messages = vec![Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text { 
                        text: format!("Summarize this 10-Q report: {}", content) 
                    }],
                }];

                let messages_request = MessagesRequestBuilder::default()
                    .messages(messages.clone())
                    .model("claude-3-sonnet-20240229".to_string())
                    .max_tokens(1000usize)
                    .build()?;

                // Send a completion request.
                let messages_response = client.messages(messages_request).await?;

                // Extract and print the assistant's response
                if let Some(content) = messages_response.content.first() {
                    if let ContentBlock::Text { text } = content {
                        println!("Claude's summary: {}", text);
                    }
                }
            },
            Err(e) => println!("Error retrieving 10-Q: {}", e),
        }

        println!(); // Add a blank line for readability
    }

    println!("Goodbye!");
    Ok(())
}

use anyhow::Result;
use anthropic::{Client, Message};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let api_key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set in the environment");
    let client = Client::new(api_key);

    // Example: Send a simple message to Claude
    let response = client
        .message()
        .create(Message {
            model: "claude-3-opus-20240229".to_string(),
            max_tokens: 1000,
            messages: vec![anthropic::types::MessageParam {
                role: "user".to_string(),
                content: "Hello, Claude!".to_string(),
            }],
            ..Default::default()
        })
        .await?;

    println!("Claude's response: {}", response.content[0].text);

    // TODO: Implement file handling and more complex interactions

    Ok(())
}

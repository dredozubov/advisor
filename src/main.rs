use anyhow::Result;
use claude_rs::Claude;
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    let api_key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");
    let claude = Claude::new(&api_key);

    // Example: Send a simple message to Claude
    let response = claude.send_message("Hello, Claude!").await?;
    println!("Claude's response: {}", response);

    // TODO: Implement file handling and more complex interactions

    Ok(())
}

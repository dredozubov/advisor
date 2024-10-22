use crate::edgar::index::{update_full_index_feed, Config};
use anyhow::Result;

pub async fn eval(input: &str, config: &Config) -> Result<String> {
    // Parse the input and extract relevant information
    let tokens: Vec<&str> = input.trim().split_whitespace().collect();

    if tokens.is_empty() {
        return Ok("Please provide a valid input.".to_string());
    }

    match tokens[0].to_lowercase().as_str() {
        "index" => {
            update_full_index_feed(config).await?;
            Ok("Index updated successfully.".to_string())
        }
        _ => Ok(format!("Unknown command: {}", input)),
    }
}

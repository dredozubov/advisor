use anyhow::Result;
use crate::edgar::index::{Config, update_full_index_feed};
use crate::edgar::tickers::fetch_tickers;

pub async fn eval(input: &str, config: &Config) -> Result<String> {
    match input.trim().to_lowercase().as_str() {
        "fetch tickers" => {
            let tickers = fetch_tickers().await?;
            Ok(format!("Fetched {} tickers", tickers.len()))
        }
        "update index" => {
            update_full_index_feed(config).await?;
            Ok("Full index feed updated successfully.".to_string())
        }
        _ => Ok(format!("Evaluated: {}", input))
    }
}

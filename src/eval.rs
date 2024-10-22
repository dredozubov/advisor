use anyhow::Result;
use crate::edgar::index::{Config, update_full_index_feed};
use crate::edgar::tickers::{fetch_tickers, load_tickers};

pub async fn eval(input: &str, config: &Config) -> Result<String> {
    // Parse the input and extract relevant information
    let tokens: Vec<&str> = input.trim().split_whitespace().collect();
    
    if tokens.is_empty() {
        return Ok("Please provide a valid input.".to_string());
    }

    match tokens[0].to_lowercase().as_str() {
        "ticker" | "tickers" => {
            if tokens.len() > 1 {
                let ticker = tokens[1].to_uppercase();
                let tickers = load_tickers().unwrap_or_else(|_| fetch_tickers().await.unwrap());
                if let Some(ticker_data) = tickers.iter().find(|&t| t.0 == ticker) {
                    Ok(format!("Ticker: {}, Company: {}, CIK: {}", ticker_data.0, ticker_data.1, ticker_data.2))
                } else {
                    Ok(format!("Ticker {} not found", ticker))
                }
            } else {
                let tickers = load_tickers().unwrap_or_else(|_| fetch_tickers().await.unwrap());
                Ok(format!("Available tickers: {}", tickers.len()))
            }
        }
        "index" => {
            update_full_index_feed(config).await?;
            Ok("Index updated successfully.".to_string())
        }
        _ => Ok(format!("Unknown command: {}", input))
    }
}

use anyhow::Result;
use chrono::NaiveDate;
use mime::APPLICATION_JSON;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use url::Url;

use crate::utils::{http::fetch_and_save, rate_limit::RateLimiter};

use crate::utils::dirs::EARNINGS_DIR;
const USER_AGENT: &str = "software@example.com";

#[derive(Debug, Serialize, Deserialize)]
pub struct Transcript {
    pub ticker: String,
    pub date: NaiveDate,
    pub quarter: String,
    pub year: i32,
    pub content: String,
}


pub async fn fetch_transcript(
    client: &Client,
    ticker: &str,
    date: NaiveDate,
) -> Result<Transcript> {
    crate::utils::dirs::ensure_earnings_dirs()?;

    let url = format!(
        "https://api.example.com/earnings/{}/{}",
        ticker,
        date.format("%Y-%m-%d")
    );

    let filepath = PathBuf::from(EARNINGS_DIR)
        .join(ticker)
        .join(format!("{}.json", date.format("%Y-%m-%d")));

    // Create ticker directory if it doesn't exist
    if let Some(parent) = filepath.parent() {
        fs::create_dir_all(parent)?;
    }

    // Fetch and save transcript
    fetch_and_save(
        client,
        &Url::parse(&url)?,
        &filepath,
        USER_AGENT,
        APPLICATION_JSON,
        RateLimiter::earnings(),
    )
    .await?;

    // Read and parse the saved transcript
    let content = fs::read_to_string(&filepath)?;
    let transcript: Transcript = serde_json::from_str(&content)?;

    Ok(transcript)
}

pub async fn fetch_transcripts(
    client: &Client,
    ticker: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<Transcript>> {
    let mut transcripts = Vec::new();

    // Create a bounded channel
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    // Spawn tasks to fetch each transcript
    let mut handles = Vec::new();
    let mut current_date = start_date;
    while current_date <= end_date {
        let tx = tx.clone();
        let client = client.clone();
        let ticker = ticker.to_string();
        let date = current_date;

        let handle = tokio::spawn(async move {
            match fetch_transcript(&client, &ticker, date).await {
                Ok(transcript) => {
                    let _ = tx.send(Some(transcript)).await;
                }
                Err(e) => {
                    log::error!("Error fetching transcript: {}", e);
                    let _ = tx.send(None).await;
                }
            }
        });
        handles.push(handle);

        current_date = current_date
            .checked_add_signed(chrono::Duration::days(90))
            .unwrap_or(end_date);
    }

    // Drop the original sender
    drop(tx);

    // Collect results
    while let Some(result) = rx.recv().await {
        if let Some(transcript) = result {
            transcripts.push(transcript);
        }
    }

    // Wait for all tasks to complete
    for handle in handles {
        if let Err(e) = handle.await {
            log::error!("Task join error: {}", e);
        }
    }

    Ok(transcripts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_fetch_transcript() {
        let client = Client::new();
        let ticker = "AAPL";
        let date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();

        let result = fetch_transcript(&client, ticker, date).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_transcripts() {
        let client = Client::new();
        let ticker = "AAPL";
        let start_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();

        let result = fetch_transcripts(&client, ticker, start_date, end_date).await;
        assert!(result.is_ok());
    }
}
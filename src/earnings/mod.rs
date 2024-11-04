mod query;
pub use query::Query;

use anyhow::{anyhow, Result}; 
use chrono::{Datelike, NaiveDate};
use once_cell::sync::OnceCell;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::utils::rate_limit::RateLimiter;

static RATE_LIMITER: OnceCell<RateLimiter> = OnceCell::new();

fn rate_limiter() -> &'static RateLimiter {
    RATE_LIMITER.get_or_init(|| RateLimiter::new(10))
}

use crate::utils::dirs::EARNINGS_DIR;
const USER_AGENT: &str = "software@example.com";
const API_BASE_URL: &str = "https://discountingcashflows.com/api/transcript";

#[derive(Debug, Serialize, Deserialize)]
pub struct Transcript {
    pub symbol: String,
    #[serde(rename = "date")]
    pub timestamp: String,
    pub quarter: i32,
    pub year: i32,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TranscriptResponse {
    #[serde(rename = "Content")]
    content: Vec<Transcript>
}

pub async fn fetch_transcript(
    client: &Client,
    ticker: &str,
    date: NaiveDate,
) -> Result<Transcript> {
    crate::utils::dirs::ensure_earnings_dirs()?;

    let quarter = get_quarter_for_date(date);
    let year = date.year();

    let url = format!(
        "{}/{}/{}/{}/",
        API_BASE_URL,
        ticker,
        quarter,
        year
    );
    
    log::debug!("Earnings API Request URL: {}", url);
    log::debug!("Earnings API Headers: User-Agent: {}", USER_AGENT);

    let filepath = PathBuf::from(EARNINGS_DIR)
        .join(ticker)
        .join(format!("{}_{}_Q{}.json", ticker, year, quarter));

    // Create ticker directory if it doesn't exist
    if let Some(parent) = filepath.parent() {
        fs::create_dir_all(parent)?;
    }

    // Fetch and save transcript
    crate::utils::http::fetch_and_save(
        client,
        &url::Url::parse(&url)?,
        &filepath,
        USER_AGENT,
        mime::APPLICATION_JSON,
        rate_limiter(),
    )
    .await?;

    // Read and parse the saved transcript
    let content = fs::read_to_string(&filepath)?;
    log::debug!("Raw transcript response content: {}", content);
    
    let response: TranscriptResponse = match serde_json::from_str(&content) {
        Ok(r) => r,
        Err(e) => {
            log::error!(
                "Failed to parse transcript JSON for {} {} Q{}\nError: {}\nContent: {}",
                ticker,
                year,
                quarter,
                e,
                content
            );
            return Err(anyhow!(
                "Failed to parse transcript for {} {} Q{}: {}",
                ticker,
                year,
                quarter,
                e
            ));
        }
    };

    // Get the first transcript from the array
    let transcript = response.content.into_iter().next().ok_or_else(|| {
        anyhow!("No transcript found in response for {} {} Q{}", ticker, year, quarter)
    })?;

    // Validate the response
    if transcript.symbol != ticker || 
       transcript.year != year || 
       transcript.quarter != quarter {
        return Err(anyhow!(
            "Mismatched transcript data: expected {}/{}/Q{}, got {}/{}/Q{}",
            ticker,
            year,
            quarter,
            transcript.symbol,
            transcript.year,
            transcript.quarter
        ));
    }

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

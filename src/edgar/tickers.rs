use crate::edgar::index::Config;
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use reqwest::{Client, Url};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const TICKER_URL: &str = "https://www.sec.gov/files/company_tickers.json";

pub type TickerData = (Ticker, String, String); // (ticker, company name, CIK)

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ticker(String);

impl Ticker {
    pub fn new(ticker: String) -> Result<Self> {
        let uppercase_ticker = ticker.to_uppercase();
        if uppercase_ticker.is_empty() {
            return Err(anyhow!("Ticker cannot be empty"));
        }
        if !uppercase_ticker.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(anyhow!("Ticker must contain only alphanumeric characters"));
        }
        Ok(Ticker(uppercase_ticker))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Ticker {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Ticker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

static USER_AGENT: Lazy<String> = Lazy::new(|| {
    Config::load()
        .map(|config| config.user_agent)
        .unwrap_or_else(|_| "software@example.com".to_string())
});

pub async fn fetch_tickers() -> Result<Vec<TickerData>> {
    let client = Client::new();
    let url = Url::parse(TICKER_URL)?;
    let path = Path::new("edgar_data/tickers.json");

    if !path.exists() {
        super::utils::fetch_and_save(&client, &url, path, &USER_AGENT).await?;
    }

    load_tickers()
}

pub fn load_tickers() -> Result<Vec<TickerData>> {
    let path = Path::new("edgar_data/tickers.json");
    if path.exists() {
        let json_string = fs::read_to_string(path)?;
        let json: HashMap<String, Value> = serde_json::from_str(&json_string)?;

        json.values()
            .map(|v| {
                let ticker = Ticker::new(v["ticker"].as_str().unwrap().to_string())?;
                Ok((
                    ticker,
                    v["title"].as_str().unwrap().to_string(),
                    format!("{:010}", v["cik_str"].as_u64().unwrap()),
                ))
            })
            .collect()
    } else {
        Err(anyhow!(
            "Tickers file not found. Run fetch_latest_tickers() first."
        ))
    }
}

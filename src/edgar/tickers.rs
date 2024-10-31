use anyhow::{anyhow, Result};
use mime::APPLICATION_JSON;
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
        if !uppercase_ticker
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(anyhow!(
                "Ticker must contain only alphanumeric characters or hyphens: {}",
                ticker
            ));
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

static USER_AGENT: &str = "software@example.com";

pub async fn fetch_tickers() -> Result<Vec<TickerData>> {
    log::debug!("Fetching tickers from SEC");
    let client = Client::new();
    let url = Url::parse(TICKER_URL)?;
    let path = Path::new("data/edgar/tickers.json");
    log::debug!("Checking for existing tickers file at {:?}", path);

    if !path.exists() {
        log::debug!("Tickers file not found, downloading from SEC");
        super::utils::fetch_and_save(&client, &url, path, USER_AGENT, APPLICATION_JSON).await?;
        log::debug!("Successfully downloaded tickers file");
    } else {
        log::debug!("Using existing tickers file");
    }

    load_tickers()
}

pub fn load_tickers() -> Result<Vec<TickerData>> {
    let path = Path::new("data/edgar/tickers.json");
    log::debug!("Loading tickers from {:?}", path);
    if path.exists() {
        log::debug!("Reading tickers file");
        let json_string = fs::read_to_string(path)?;
        log::debug!("Parsing JSON data");
        let json: HashMap<String, Value> = serde_json::from_str(&json_string)?;
        log::debug!("Found {} ticker entries", json.len());

        let result: Result<Vec<TickerData>> = json
            .values()
            .map(|v| {
                let ticker_str = v["ticker"].as_str().unwrap().trim().to_string();
                let ticker = Ticker::new(ticker_str)?;
                Ok((
                    ticker,
                    v["title"].as_str().unwrap().to_string(),
                    format!("{:010}", v["cik_str"].as_u64().unwrap()),
                ))
            })
            .collect();
        log::debug!("Finished processing all tickers");
        result
    } else {
        Err(anyhow!(
            "Tickers file not found. Run fetch_latest_tickers() first."
        ))
    }
}

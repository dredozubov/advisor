use anyhow::Result;
use once_cell::sync::Lazy;
use reqwest::{Client, Url};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use crate::edgar::utils::fetch_and_save;

const TICKER_URL: &str = "https://www.sec.gov/files/company_tickers.json";

pub type TickerData = (String, String, String); // (ticker, company name, CIK)

pub async fn fetch_latest_tickers() -> Result<Vec<TickerData>> {
    let client = Client::new();
    let url = Url::parse(TICKER_URL)?;
    let path = Path::new("edgar_data/tickers.json");
    let user_agent = "Mozilla/5.0";

    fetch_and_save(&client, &url, path, user_agent).await?;

    // Load and parse the saved JSON file
    let json_string = fs::read_to_string(path)?;
    let json: HashMap<String, Value> = serde_json::from_str(&json_string)?;

    let tickers: Vec<TickerData> = json
        .values()
        .map(|v| {
            (
                v["ticker"].as_str().unwrap().to_string(),
                v["title"].as_str().unwrap().to_string(),
                format!("{:010}", v["cik_str"].as_u64().unwrap()),
            )
        })
        .collect();

    Ok(tickers)
}

pub fn load_tickers() -> Result<Vec<TickerData>> {
    let path = Path::new("edgar_data/tickers.json");
    if path.exists() {
        let json_string = fs::read_to_string(path)?;
        let json: HashMap<String, Value> = serde_json::from_str(&json_string)?;

        Ok(json
            .values()
            .map(|v| {
                (
                    v["ticker"].as_str().unwrap().to_string(),
                    v["title"].as_str().unwrap().to_string(),
                    format!("{:010}", v["cik_str"].as_u64().unwrap()),
                )
            })
            .collect())
    } else {
        Err(anyhow::anyhow!(
            "Tickers file not found. Run fetch_latest_tickers() first."
        ))
    }
}

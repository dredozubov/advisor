use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use log::{error, info};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

// Hardcoded values
const FILING_DATA_DIR: &str = "edgar_data/filings";
const EDGAR_DATA_URL: &str = "https://data.sec.gov";
const USER_AGENT: &str = "software@example.com";

#[derive(Debug, Serialize, Deserialize)]
pub struct CompanyInfo {
    pub cik: String,
    pub name: String,
    pub tickers: Vec<String>,
    pub exchanges: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilingEntry {
    pub accessionNumber: Vec<String>,
    pub filingDate: Vec<String>,
    pub reportDate: Vec<String>,
    pub acceptanceDateTime: Vec<String>,
    pub act: Vec<String>,
    pub form: Vec<String>,
    pub fileNumber: Vec<String>,
    pub filmNumber: Vec<String>, 
    pub items: Vec<String>,
    pub size: Vec<i64>,
    pub isXBRL: Vec<i64>,
    pub isInlineXBRL: Vec<i64>,
    pub primaryDocument: Vec<String>,
    pub primaryDocDescription: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilingFile {
    pub name: String,
    pub filingCount: i64,
    pub filingFrom: String,
    pub filingTo: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilingsData {
    pub recent: FilingEntry,
    pub files: Vec<FilingFile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompanyFilings {
    pub cik: String,
    pub entityType: String,
    pub sic: String,
    pub sicDescription: String,
    pub name: String,
    pub tickers: Vec<String>,
    pub exchanges: Vec<String>,
    pub filings: FilingsData,
}

pub async fn get_company_filings(client: &Client, cik: &str) -> Result<CompanyFilings> {
    // Ensure CIK is 10 digits with leading zeros
    let padded_cik = format!("{:0>10}", cik);
    let url = format!("{}/submissions/CIK{}.json", EDGAR_DATA_URL, padded_cik);

    info!("Fetching company filings from {}", url);

    // Create filings directory if it doesn't exist
    fs::create_dir_all(FILING_DATA_DIR)?;
    
    let filepath = PathBuf::from(FILING_DATA_DIR).join(format!("CIK{}.json", padded_cik));

    if !filepath.exists() {
        
        super::utils::fetch_and_save(
            client,
            &Url::parse(&url)?,
            &filepath,
            USER_AGENT,
        ).await?;
    }

    let content = fs::read_to_string(&filepath)?;
    let filings: CompanyFilings = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse filings JSON: {}", e))?;

    Ok(filings)
}

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, NaiveDateTime};
use log::{error, info};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

use super::report::ReportType;
use super::tickers::Ticker;

// Hardcoded values
pub const FILING_DATA_DIR: &str = "edgar_data/filings";
pub const EDGAR_DATA_URL: &str = "https://data.sec.gov";
pub const USER_AGENT: &str = "software@example.com";

#[derive(Debug, Serialize, Deserialize)]
pub struct CompanyInfo {
    pub cik: String,
    pub name: String,
    pub tickers: Vec<String>,
    pub exchanges: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn validate_json(content: &str) {
        match serde_json::from_str::<serde_json::Value>(content) {
            Ok(raw_json) => {
                println!(
                    "JSON is valid. Content type: {}",
                    if raw_json.is_object() {
                        "object"
                    } else {
                        "not an object"
                    }
                );

                if let Some(obj) = raw_json.as_object() {
                    println!("Top-level keys: {:?}", obj.keys().collect::<Vec<_>>());
                }
            }
            Err(e) => {
                println!("Invalid JSON structure: {}", e);
                let line = e.line();
                let column = e.column();
                let start = content.len().saturating_sub(100);
                let end = content.len().min(start + 200);
                println!("Error at line {}, column {}", line, column);
                println!("Error context: {}", &content[start..end]);
                panic!("Invalid JSON in test file: {}", e);
            }
        }
    }

    fn get_test_companies() -> Vec<String> {
        let test_dir = PathBuf::from("src/edgar/tests");
        std::fs::read_dir(test_dir)
            .expect("Failed to read test directory")
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if entry.file_type().ok()?.is_dir() {
                    Some(entry.file_name().to_string_lossy().into_owned())
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn test_parse_filing_entry() {
        for company in get_test_companies() {
            let file_path = PathBuf::from(format!(
                "src/edgar/tests/{}/filing_entry.json",
                company
            ));
            
            println!("Testing filing entry for company: {}", company);
            
            let content = std::fs::read_to_string(&file_path)
                .expect(&format!("Failed to read test file for {}", company));

            validate_json(&content);

            let entry: FilingEntry = serde_json::from_str(&content)
                .expect(&format!("Failed to parse filing entry JSON for {}", company));

            // Verify the entry has valid data
            assert!(!entry.accession_number.is_empty(), "Company {} has empty accession numbers", company);
            assert!(!entry.filing_date.is_empty(), "Company {} has empty filing dates", company);
            assert!(!entry.report_type.is_empty(), "Company {} has empty report types", company);
        }
    }

    #[test]
    fn test_parse_company_filings() {
        for company in get_test_companies() {
            let file_path = PathBuf::from(format!(
                "src/edgar/tests/{}/filing.json",
                company
            ));
            
            println!("Testing company filing for: {}", company);
            
            let content = std::fs::read_to_string(&file_path)
                .expect(&format!("Failed to read test file for {}", company));

            validate_json(&content);

            let filings: CompanyFilings = serde_json::from_str(&content)
                .expect(&format!("Failed to parse company filings JSON for {}", company));

            // Verify basic company info is present
            assert!(!filings.cik.is_empty(), "Company {} has empty CIK", company);
            assert!(!filings.name.is_empty(), "Company {} has empty name", company);
            assert!(!filings.tickers.is_empty(), "Company {} has empty tickers", company);
            assert!(!filings.exchanges.is_empty(), "Company {} has empty exchanges", company);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilingEntry {
    #[serde(rename = "accessionNumber")]
    pub accession_number: Vec<String>,
    #[serde(rename = "filingDate")]
    pub filing_date: Vec<String>,
    #[serde(rename = "reportDate")]
    pub report_date: Vec<Option<String>>,
    #[serde(rename = "acceptanceDateTime")]
    pub acceptance_date_time: Vec<String>,
    pub act: Vec<String>,
    #[serde(rename = "form")]
    pub report_type: Vec<String>,
    #[serde(rename = "fileNumber")]
    pub file_number: Vec<String>,
    #[serde(rename = "filmNumber")]
    pub film_number: Vec<String>,
    pub items: Vec<String>,
    pub size: Vec<i32>,
    #[serde(rename = "isXBRL")]
    pub is_xbrl: Vec<i32>,
    #[serde(rename = "isInlineXBRL")]
    pub is_inline_xbrl: Vec<i32>,
    #[serde(rename = "primaryDocument")]
    pub primary_document: Vec<String>,
    #[serde(rename = "primaryDocDescription")]
    pub primary_doc_description: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilingFile {
    pub name: String,
    #[serde(rename = "filingCount")]
    pub filing_count: i64,
    #[serde(rename = "filingFrom")]
    pub filing_from: String,
    #[serde(rename = "filingTo")]
    pub filing_to: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilingsData {
    pub recent: FilingEntry,
    pub files: Vec<FilingFile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompanyFilings {
    pub cik: String,
    #[serde(rename = "entityType")]
    pub entity_type: String,
    pub sic: String,
    #[serde(rename = "sicDescription")]
    pub sic_description: String,
    pub name: String,
    pub tickers: Vec<String>,
    pub exchanges: Vec<String>,
    pub filings: FilingsData,
}

pub async fn get_company_filings(
    client: &Client,
    cik: &str,
    limit: Option<usize>,
) -> Result<CompanyFilings> {
    // Ensure CIK is 10 digits with leading zeros
    let padded_cik = format!("{:0>10}", cik);
    let initial_url = format!("{}/submissions/CIK{}.json", EDGAR_DATA_URL, padded_cik);

    info!("Fetching company filings from {}", initial_url);

    // Create filings directory if it doesn't exist
    fs::create_dir_all(FILING_DATA_DIR)?;

    let mut all_filings = Vec::new();
    let mut fetched_count = 0;
    let mut current_url = initial_url;
    let mut additional_files = Vec::new();

    loop {
        let filepath = PathBuf::from(FILING_DATA_DIR)
            .join(format!("CIK{}_{}.json", padded_cik, fetched_count));

        if !filepath.exists() {
            super::utils::fetch_and_save(client, &Url::parse(&current_url)?, &filepath, USER_AGENT)
                .await?;
        }

        let mut content = fs::read_to_string(&filepath)?;
        log::debug!(
            "Read file content from {:?}, length: {}",
            filepath,
            content.len()
        );

        log::debug!("fetched_count: {}", fetched_count);
        // Handle first page differently than subsequent pages
        if fetched_count == 0 {
            log::debug!("Parsing initial response JSON");

            // Now try to parse into our structure
            let response: CompanyFilings = serde_json::from_str(&content).map_err(|e| {
                log::error!("Failed to parse into CompanyFilings: {}", e);
                // Log the first 1000 characters of content for debugging
                log::debug!("Content preview: {}", &content);
                anyhow!("Failed to parse initial filings JSON: {}", e)
            })?;
            log::debug!("Successfully parsed initial response");

            all_filings.push(response.filings.recent.clone());
            additional_files = response.filings.files.clone();
        } else {
            log::debug!("Parsing subsequent page JSON");
            let page_filings: FilingEntry = serde_json::from_str(&content).map_err(|e| {
                log::error!("JSON parse error on subsequent page: {}", e);
                log::debug!("Problematic JSON content: {}", content);
                anyhow!("Failed to parse page filings JSON: {}", e)
            })?;
            log::debug!("Successfully parsed subsequent page");
            all_filings.push(page_filings);
        }

        fetched_count += 1;

        // Check if we've hit the requested limit
        if let Some(limit) = limit {
            if fetched_count >= limit {
                break;
            }
        }

        // Check if there are more pages to fetch
        if additional_files.is_empty() {
            break;
        }
        log::debug!("additional files: {:?}", additional_files);

        // Get next page URL
        let next_page = additional_files.remove(0);
        current_url = format!("{}/submissions/{}", EDGAR_DATA_URL, next_page.name);
    }

    // Get the initial response which contains company info
    let content = fs::read_to_string(
        PathBuf::from(FILING_DATA_DIR).join(format!("CIK{}_{}.json", padded_cik, 0)),
    )?;

    let mut initial_response: CompanyFilings = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse initial filings JSON: {}", e))?;

    // Merge all filing entries into the initial response's recent filings
    let mut merged = FilingEntry {
        accession_number: Vec::new(),
        filing_date: Vec::new(),
        report_date: Vec::new(),
        acceptance_date_time: Vec::new(),
        act: Vec::new(),
        report_type: Vec::new(),
        file_number: Vec::new(),
        film_number: Vec::new(),
        items: Vec::new(),
        size: Vec::new(),
        is_xbrl: Vec::new(),
        is_inline_xbrl: Vec::new(),
        primary_document: Vec::new(),
        primary_doc_description: Vec::new(),
    };

    for filing in all_filings {
        merged.accession_number.extend(filing.accession_number);
        merged.filing_date.extend(filing.filing_date);
        merged.report_date.extend(filing.report_date);
        merged
            .acceptance_date_time
            .extend(filing.acceptance_date_time);
        merged.act.extend(filing.act);
        merged.report_type.extend(filing.report_type);
        merged.file_number.extend(filing.file_number);
        merged.film_number.extend(filing.film_number);
        merged.items.extend(filing.items);
        merged.size.extend(filing.size);
        merged.is_xbrl.extend(filing.is_xbrl);
        merged.is_inline_xbrl.extend(filing.is_inline_xbrl);
        merged.primary_document.extend(filing.primary_document);
        merged
            .primary_doc_description
            .extend(filing.primary_doc_description);
    }

    // Update the initial response with merged filings
    initial_response.filings.recent = merged;

    Ok(initial_response)
}

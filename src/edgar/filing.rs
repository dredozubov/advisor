use anyhow::Result;
use chardet::detect;
use encoding_rs::Encoding;
use encoding_rs_io::DecodeReaderBytesBuilder;
use html_escape::decode_html_entities;
use log::{error, info};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::io::{BufReader, Read};
use std::path::Path;
use url::Url;
use uuencode::uudecode;

// Hardcoded values
const FILING_DATA_DIR: &str = "/path/to/filing/data";
const EDGAR_ARCHIVES_URL: &str = "https://www.sec.gov/Archives/";
const USER_AGENT: &str = "YourCompanyName YourAppName YourEmail";

pub struct Filing {
    content: String,
    metadata: HashMap<String, String>,
}

impl Filing {
    pub fn new(content: String, metadata: HashMap<String, String>) -> Self {
        Filing { content, metadata }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
}

pub fn generate_filepaths(sec_filing: &HashMap<String, String>) -> HashMap<String, String> {
    // ... (keep the existing implementation)
}

pub async fn process_filing(
    client: &Client,
    filing_meta: &HashMap<String, String>,
) -> Result<Filing> {
    let filing_filepaths = generate_filepaths(filing_meta);

    let filing_url = Url::parse(&filing_filepaths["filing_url"])?;
    let filepath = Path::new(&filing_filepaths["filing_filepath"]);

    super::utils::fetch_and_save(client, &filing_url, filepath, USER_AGENT).await?;

    let filing_content = extract(&filing_filepaths)?;

    // Combine all document contents into a single string
    let content = filing_content.values()
        .filter_map(|v| v.as_object())
        .filter_map(|obj| obj.get("content"))
        .filter_map(|v| v.as_str())
        .collect::<Vec<&str>>()
        .join("\n\n");

    // Extract metadata
    let metadata = filing_content.values()
        .filter_map(|v| v.as_object())
        .flat_map(|obj| obj.iter())
        .filter(|(k, _)| *k != "content")
        .map(|(k, v)| (k.clone(), v.to_string()))
        .collect();

    Ok(Filing::new(content, metadata))
}

pub fn extract(
    filing_json: &HashMap<String, String>,
) -> Result<HashMap<String, serde_json::Value>> {
    // ... (keep the existing implementation)
}

pub fn extract_complete_submission_filing(
    filepath: &str,
    output_directory: Option<&Path>,
) -> Result<HashMap<String, serde_json::Value>> {
    // ... (keep the existing implementation, but add the content to the filing_document)
    // Add this line after processing the raw_text:
    // filing_document.insert("content".to_string(), raw_text.clone());
}

pub fn header_parser(raw_html: &str) -> Result<Vec<(String, String)>> {
    // ... (keep the existing implementation)
}

fn format_filename(filename: &str) -> String {
    // ... (keep the existing implementation)
}

fn file_size(filepath: &Path) -> Result<String> {
    // ... (keep the existing implementation)
}

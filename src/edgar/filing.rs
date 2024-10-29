use anyhow::{anyhow, Result};
use chardet::detect;
use chrono::NaiveDate;
use encoding_rs::Encoding;
use encoding_rs_io::DecodeReaderBytesBuilder;
use html_escape::decode_html_entities;
use log::{error, info, warn};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

use super::query::Query;
use super::report::ReportType;

#[derive(Debug, Clone)]
pub struct Filing {
    pub accession_number: String,
    pub filing_date: NaiveDate,
    pub report_date: Option<String>,
    pub acceptance_date_time: String,
    pub act: String,
    pub report_type: String,
    pub file_number: String,
    pub film_number: String,
    pub items: String,
    pub size: i32,
    pub is_xbrl: i32,
    pub is_inline_xbrl: i32,
    pub primary_document: String,
    pub primary_doc_description: String,
}

impl Filing {
    fn matches_report_type(&self, report_types: &[ReportType]) -> bool {
        // self - from Filing
        report_types.iter().any(|rt| {
            let report_type = &self.report_type;
            let amendment_type = format!("{}/A", report_type); // Handle amendments

            report_type == &rt.to_string() || report_type == &amendment_type
        })
    }
}

fn process_filing_entries(entry: &FilingEntry, query: &Query) -> Vec<Filing> {
    let mut filings = Vec::new();

    // Zip all the vectors together and process each record
    for i in 0..entry.accession_number.len() {
        let filing = Filing {
            accession_number: entry.accession_number[i].clone(),
            filing_date: entry.filing_date[i],
            report_date: entry.report_date[i].clone(),
            acceptance_date_time: entry.acceptance_date_time[i].clone(),
            act: entry.act[i].clone(),
            report_type: entry.report_type[i].clone(),
            file_number: entry.file_number[i].clone(),
            film_number: entry.film_number[i].clone(),
            items: entry.items[i].clone(),
            size: entry.size[i],
            is_xbrl: entry.is_xbrl[i],
            is_inline_xbrl: entry.is_inline_xbrl[i],
            primary_document: entry.primary_document[i].clone(),
            primary_doc_description: entry.primary_doc_description[i].clone(),
        };

        // Construct document URL
        let base = "https://www.sec.gov/Archives/edgar/data";
        let cik = format!("{:0>10}", query.tickers[0]); // Assuming the first ticker's CIK is used
        let accession_number = filing.accession_number.replace("-", "");
        let document_url = format!(
            "{}/{}/{}/{}",
            base, cik, accession_number, filing.primary_document
        );

        log::info!("Constructed document URL: {}", document_url);

        // Check if filing matches query criteria
        if filing.matches_report_type(&query.report_types)
            && filing.filing_date >= query.start_date
            && filing.filing_date <= query.end_date
        {
            filings.push(filing);
        }
    }

    filings
}

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
            let file_path = PathBuf::from(format!("src/edgar/tests/{}/filing_entry.json", company));

            println!("Testing filing entry for company: {}", company);

            let content = std::fs::read_to_string(&file_path)
                .expect(&format!("Failed to read test file for {}", company));

            validate_json(&content);

            let entry: FilingEntry = serde_json::from_str(&content).expect(&format!(
                "Failed to parse filing entry JSON for {}",
                company
            ));

            // Verify the entry has valid data
            assert!(
                !entry.accession_number.is_empty(),
                "Company {} has empty accession numbers",
                company
            );
            assert!(
                !entry.filing_date.is_empty(),
                "Company {} has empty filing dates",
                company
            );
            assert!(
                !entry.report_type.is_empty(),
                "Company {} has empty report types",
                company
            );
        }
    }

    #[test]
    fn test_parse_company_filings() {
        for company in get_test_companies() {
            let file_path = PathBuf::from(format!("src/edgar/tests/{}/filing.json", company));

            println!("Testing company filing for: {}", company);

            let content = std::fs::read_to_string(&file_path)
                .expect(&format!("Failed to read test file for {}", company));

            validate_json(&content);

            let filings: CompanyFilings = serde_json::from_str(&content).expect(&format!(
                "Failed to parse company filings JSON for {}",
                company
            ));

            // Verify basic company info is present
            assert!(!filings.cik.is_empty(), "Company {} has empty CIK", company);
            assert!(
                !filings.name.is_empty(),
                "Company {} has empty name",
                company
            );
            assert!(
                !filings.tickers.is_empty(),
                "Company {} has empty tickers",
                company
            );
            assert!(
                !filings.exchanges.is_empty(),
                "Company {} has empty exchanges",
                company
            );
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilingEntry {
    #[serde(rename = "accessionNumber")]
    pub accession_number: Vec<String>,
    #[serde(rename = "filingDate")]
    pub filing_date: Vec<NaiveDate>,
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
    let rate_limiter = super::rate_limiter::RateLimiter::global();
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
            // Acquire rate limit permit
            let _permit = rate_limiter.acquire().await;

            match super::utils::fetch_and_save(
                client,
                &Url::parse(&current_url)?,
                &filepath,
                USER_AGENT,
            )
            .await
            {
                Ok(_) => {
                    log::debug!("Successfully fetched and saved filing from {}", current_url);
                }
                Err(e) => {
                    // If the file exists despite an error, try to use it anyway
                    if !filepath.exists() {
                        error!(
                            "Failed to fetch filings from {} and no local file exists: {}",
                            current_url, e
                        );
                        return Err(anyhow!("Failed to fetch filings: {}", e));
                    }
                    log::warn!("Error fetching from {} but local file exists, attempting to use cached version: {}", current_url, e);
                }
            }
        }

        let content = fs::read_to_string(&filepath).map_err(|e| {
            error!("Failed to read filing file {:?}: {}", filepath, e);
            anyhow!("Failed to read filing file: {}", e)
        })?;

        // Validate JSON before parsing
        if let Err(e) = serde_json::from_str::<serde_json::Value>(&content) {
            error!("Invalid JSON in response from {}: {}", current_url, e);
            error!("Content length: {}", content.len());
            if content.len() > 1000 {
                error!("First 1000 chars: {}", &content[..1000]);
                error!("Last 1000 chars: {}", &content[content.len() - 1000..]);
            } else {
                error!("Full content: {}", &content);
            }
            // Delete potentially corrupted file
            if let Err(e) = fs::remove_file(&filepath) {
                warn!("Failed to remove corrupted file {:?}: {}", filepath, e);
            }
            return Err(anyhow!("Invalid JSON response from SEC: {}", e));
        }
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
                error!("Failed to parse into CompanyFilings: {}", e);
                // Log the problematic content for debugging
                error!("Content length: {}", content.len());
                if content.len() > 1000 {
                    error!("Content preview (first 1000 chars): {}", &content[..1000]);
                    error!(
                        "Content preview (last 1000 chars): {}",
                        &content[content.len() - 1000..]
                    );
                } else {
                    error!("Full content: {}", &content);
                }
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

pub async fn fetch_matching_filings(
    client: &Client,
    query: &Query,
) -> Result<HashMap<String, Filing>> {
    // Fetch tickers to get CIKs
    let tickers = super::tickers::fetch_tickers().await?;

    // Find the CIK for the first ticker in the query
    let cik = tickers
        .iter()
        .find(|(ticker, _, _)| ticker.as_str() == query.tickers[0])
        .map(|(_, _, cik)| cik)
        .ok_or_else(|| anyhow!("CIK not found for ticker: {}", query.tickers[0]))?;

    // Fetch filings using the CIK
    let filings = get_company_filings(client, cik, None).await?;
    let matching_filings = process_filing_entries(&filings.filings.recent, query);

    let rate_limiter = super::rate_limiter::RateLimiter::global();

    // Create the base directory if it doesn't exist
    fs::create_dir_all(FILING_DATA_DIR)?;

    // Fetch and save each matching filing in parallel, respecting the rate limit
    let filing_map = Arc::new(Mutex::new(HashMap::new()));
    let mut handles = Vec::new();
    let fetch_tasks: Vec<_> = matching_filings
        .clone() // Clone matching_filings to avoid moving it
        .into_iter()
        .map(move |filing| {
            let filing_clone = filing.clone(); // Clone filing to store in the map later
            let client = client.clone();
            let cik = cik.to_string(); // Clone cik to avoid lifetime issues
            let filing_map_clone = Arc::clone(&filing_map); // Clone the Arc to avoid moving it
            let _permit = rate_limiter.acquire();

            let handle = tokio::spawn(async move {
                let filing_map_clone = filing_map.clone();
                let base = "https://www.sec.gov/Archives/edgar/data";
                let cik = format!("{:0>10}", cik); // cik is already a String
                let accession_number = filing.accession_number.replace("-", "");
                let document_url = format!(
                    "{}/{}/{}/{}",
                    base, cik, accession_number, filing.primary_document
                );

                // Create the directory structure for the filing
                let filing_dir = format!("{}/{}/{}", FILING_DATA_DIR, cik, accession_number);
                fs::create_dir_all(&filing_dir)?;

                // Define the path to save the document
                let document_path = format!("{}/{}", filing_dir, filing.primary_document);

                // Fetch and save the document
                let response = client.get(&document_url).send().await?;
                let content = response.bytes().await?;
                fs::write(&document_path, &content)?;

                log::info!("Saved filing document to {}", document_path);

                let mut map = filing_map_clone.lock().await;
                map.insert(document_path.clone(), filing_clone);
                Ok::<String, anyhow::Error>(document_path)
            });
            handles.push(handle);
            Ok::<(), anyhow::Error>(()) // Ensure the task returns a result
        })
        .collect();

    // Wait for all fetch tasks to complete
    futures::future::try_join_all(handles).await?; // Use handles instead of fetch_tasks

    // Convert Vec<Result<String>> into Result<Vec<String>>
    // Convert Vec<Result<String>> into Result<Vec<String>>

    // If all fetches succeeded, return the HashMap of file paths and filings
    result.and_then(|_| {
        Arc::try_unwrap(filing_map)
            .map_err(|_| anyhow!("Failed to unwrap Arc"))
            .map(|mutex| mutex.into_inner())
    })
}

pub fn extract_complete_submission_filing(
    filepath: &str,
    output_directory: &Path,
) -> Result<HashMap<String, serde_json::Value>> {
    log::debug!(
        "Starting extract_complete_submission_filing for file: {}",
        filepath
    );

    let elements_list = vec![
        ("FILENAME", "<FILENAME>"),
        ("TYPE", "<TYPE>"),
        ("SEQUENCE", "<SEQUENCE>"),
        ("DESCRIPTION", "<DESCRIPTION>"),
    ];

    log::debug!(
        "Checking if output directory exists: {:?}",
        output_directory
    );
    if !output_directory.exists() {
        log::debug!("Creating output directory: {:?}", output_directory);
        fs::create_dir_all(output_directory)?;
    }

    log::debug!(
        "Starting to extract and save parsed content to {:?}",
        output_directory
    );

    let xbrl_doc = Regex::new(r"<DOCUMENT>(.*?)</DOCUMENT>")?;
    let xbrl_text = Regex::new(r"<(TEXT|text)>(.*?)</(TEXT|text)>")?;

    log::debug!("Reading file: {}", filepath);
    let raw_text = match fs::read(filepath) {
        Ok(content) => {
            log::debug!(
                "Successfully read file: {} ({} bytes)",
                filepath,
                content.len()
            );
            content
        }
        Err(e) => {
            log::error!("Failed to read file: {}. Error: {}", filepath, e);
            return Err(e.into());
        }
    };
    let charenc = detect(&raw_text).0;

    log::debug!("Detected character encoding: {}", charenc);
    let mut raw_text = match File::open(filepath) {
        Ok(file) => {
            log::debug!("Successfully opened file: {}", filepath);
            DecodeReaderBytesBuilder::new()
                .encoding(Encoding::for_label(charenc.as_bytes()))
                .build(BufReader::new(file))
        }
        Err(e) => {
            log::error!("Failed to open file: {}. Error: {}", filepath, e);
            return Err(e.into());
        }
    };

    log::debug!("Starting to read and decode file content...");
    let mut raw_text_string = String::new();
    if let Err(e) = raw_text.read_to_string(&mut raw_text_string) {
        log::error!("Failed to read and decode file content. Error: {}", e);
        return Err(e.into());
    }
    log::debug!(
        "Successfully read and decoded file content ({} characters)",
        raw_text_string.len()
    );

    log::debug!("Parsing filing header...");
    let filing_header = match header_parser(&raw_text_string) {
        Ok(header) => {
            log::debug!("Successfully parsed filing header.");
            header
        }
        Err(e) => {
            log::error!("Failed to parse filing header. Error: {}", e);
            return Err(e.into());
        }
    };

    let mut filing_documents = HashMap::new();
    filing_documents.insert("header".to_string(), json!(filing_header));

    let documents: Vec<_> = xbrl_doc
        .find_iter(&raw_text_string)
        .map(|m| m.as_str())
        .collect();

    for (i, document) in documents.iter().enumerate() {
        let mut filing_document = HashMap::new();

        // Extract document information
        for (element, element_path) in &elements_list {
            if let Some(value) = document
                .split(element_path)
                .nth(1)
                .and_then(|s| s.split('<').next())
            {
                filing_document.insert(element.to_string(), value.trim().to_string());
            }
        }

        let raw_text = xbrl_text
            .captures(document)
            .and_then(|cap| cap.get(2))
            .map(|m| m.as_str())
            .unwrap_or("")
            .replace("<XBRL>", "")
            .replace("</XBRL>", "")
            .replace("<XML>", "")
            .replace("</XML>", "")
            .trim()
            .to_string();

        let _doc_num = format!("{:04}", i + 1); // Prefix with underscore to avoid warning
        let output_filename = format!(
            "{}_{}.txt",
            filing_document
                .get("TYPE")
                .unwrap_or(&"unknown_type".to_string()),
            filing_document
                .get("FILING_DATE")
                .unwrap_or(&"unknown_date".to_string())
        );
        let output_filepath = output_directory.join(&output_filename);

        log::debug!("Writing parsed content to file: {:?}", output_filepath);
        log::debug!("Writing parsed content to file: {:?}", output_filepath);
        log::debug!("Writing parsed content to file: {:?}", output_filepath);
        if let Err(e) = fs::write(&output_filepath, raw_text) {
            log::error!(
                "Failed to write parsed content to file: {:?}. Error: {}",
                output_filepath,
                e
            );
            return Err(e.into());
        }
        log::debug!(
            "Successfully wrote parsed content to file: {:?}",
            output_filepath
        );

        filing_document.insert(
            "RELATIVE_FILEPATH".to_string(),
            output_filepath.to_str().unwrap().to_string(),
        );
        filing_document.insert("DESCRIPTIVE_FILEPATH".to_string(), output_filename.clone());
        filing_document.insert("FILE_SIZE".to_string(), file_size(&output_filepath)?);
        filing_document.insert(
            "FILE_SIZE_BYTES".to_string(),
            fs::metadata(&output_filepath)?.len().to_string(),
        );

        filing_documents.insert(i.to_string(), json!(filing_document));
    }
    log::debug!("filing documents:\n {:?}", filing_documents);
    Ok(filing_documents)
}

pub fn header_parser(raw_html: &str) -> Result<Vec<(String, String)>> {
    let document = Html::parse_document(raw_html);
    let sec_header_selector = Selector::parse("sec-header").unwrap();

    let mut data = Vec::new();

    if let Some(sec_header_element) = document.select(&sec_header_selector).next() {
        let sec_header_html = sec_header_element.inner_html();
        let re = Regex::new(r"<(SEC-HEADER|sec-header)>(.*?)</(SEC-HEADER|sec-header)>")?;

        if let Some(captures) = re.captures(&sec_header_html) {
            if let Some(sec_header) = captures.get(2) {
                let split_header: Vec<&str> = sec_header.as_str().split('\n').collect();

                let mut current_group = String::new();
                for header_item in split_header.iter() {
                    let header_item = header_item.trim();
                    if !header_item.is_empty() {
                        if header_item.starts_with('<') && header_item.ends_with('>') {
                            current_group = header_item.to_string();
                        } else if !header_item.starts_with('\t') && !header_item.contains('<') {
                            if let Some(colon_index) = header_item.find(':') {
                                let key = header_item[..colon_index].trim();
                                let value =
                                    decode_html_entities(&header_item[colon_index + 1..].trim())
                                        .into_owned();
                                data.push((format!("{}:{}", current_group, key), value));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(data)
}

fn format_filename(filename: &str) -> String {
    filename
        .replace(" ", "_")
        .replace(":", "")
        .replace("__", "_")
}

fn file_size(filepath: &Path) -> Result<String> {
    let metadata = fs::metadata(filepath)?;
    let size = metadata.len();
    Ok(format!("{:.2} KB", size as f64 / 1024.0))
}

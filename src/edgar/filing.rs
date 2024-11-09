use anyhow::{anyhow, Result};
use chardet::detect;
use chrono::NaiveDate;
use encoding_rs::Encoding;
use encoding_rs_io::DecodeReaderBytesBuilder;
use log::{error, info, warn};
use mime::{APPLICATION_JSON, TEXT_XML};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use url::Url;

use crate::utils::http::fetch_and_save;

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

        log::debug!("Constructed document URL: {}", document_url);

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
use crate::utils::dirs::EDGAR_FILINGS_DIR;
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
                .unwrap_or_else(|_| panic!("Failed to read test file for {}", company));

            validate_json(&content);

            let entry: FilingEntry = serde_json::from_str(&content)
                .unwrap_or_else(|_| panic!("Failed to parse filing entry JSON for {}", company));

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
                .unwrap_or_else(|_| panic!("Failed to read test file for {}", company));

            validate_json(&content);

            let filings: CompanyFilings = serde_json::from_str(&content)
                .unwrap_or_else(|_| panic!("Failed to parse company filings JSON for {}", company));

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
    // Ensure CIK is 10 digits with leading zeros
    let padded_cik = format!("{:0>10}", cik);
    let initial_url = format!("{}/submissions/CIK{}.json", EDGAR_DATA_URL, padded_cik);

    info!("Fetching company filings from EDGAR API");
    log::debug!("EDGAR API Request URL: {}", initial_url);
    log::debug!("EDGAR API Headers: User-Agent: {}", USER_AGENT);

    crate::utils::dirs::ensure_edgar_dirs()?;

    let mut all_filings = Vec::new();
    let mut fetched_count = 0;
    let mut current_url = initial_url;
    let mut additional_files = Vec::new();

    loop {
        let filepath = PathBuf::from(EDGAR_FILINGS_DIR)
            .join(format!("CIK{}_{}.json", padded_cik, fetched_count));

        if !filepath.exists() {
            match fetch_and_save(
                client,
                &Url::parse(&current_url)?,
                &filepath,
                USER_AGENT,
                APPLICATION_JSON,
                crate::edgar::rate_limiter(),
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
        PathBuf::from(EDGAR_FILINGS_DIR).join(format!("CIK{}_{}.json", padded_cik, 0)),
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

    crate::utils::dirs::ensure_edgar_dirs()?;

    // Create a bounded channel
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    // Keep track of how many tasks we spawn
    let total_tasks = matching_filings.len();
    let mut completed_tasks = 0;

    log::info!("Matching filings: {:?}", matching_filings);

    // Spawn async tasks to fetch and save each matching filing
    let mut handles = Vec::new();
    for filing in matching_filings {
        let tx = tx.clone();
        let client = client.clone();
        let cik = cik.to_string();
        let handle = tokio::spawn(async move {
            let filing_clone = filing.clone();
            log::info!("calling fetching task");
            let base = "https://www.sec.gov/Archives/edgar/data";
            let accession_number = filing.accession_number.replace("-", "");
            // The primary_document from FilingEntry contains the original .htm file
            // We need to construct URL for the XBRL version by transforming .htm to _htm.xml
            let xbrl_document = filing.primary_document.replace(".htm", "_htm.xml");

            let document_url = format!("{}/{}/{}/{}", base, cik, accession_number, xbrl_document);
            log::debug!("EDGAR Document Request URL: {}", document_url);

            // Create the directory structure for the filing
            let filing_dir = format!("{}/{}/{}", EDGAR_FILINGS_DIR, cik, accession_number);
            fs::create_dir_all(&filing_dir)?;

            // Define the path to save the document using the XBRL document name
            let document_path = format!("{}/{}", filing_dir, xbrl_document);

            let document_url_obj = Url::parse(&document_url[..]).unwrap();
            let local_path = Path::new(&document_path[..]);
            log::info!("Fetching: {}", document_url);

            // Fetch and save the document with XML content type
            let result = fetch_and_save(
                &client,
                &document_url_obj,
                local_path,
                USER_AGENT,
                TEXT_XML,
                crate::edgar::rate_limiter(),
            )
            .await;

            if let Err(e) = result {
                log::error!("Error processing filing: {}", e);
                // Still send a completion signal even on error
                let _ = tx.send(("".to_string(), filing_clone)).await;
                return Ok(());
            }

            log::info!("Saved filing document to {}", document_path);

            tx.send((document_path, filing)).await?;
            Ok::<(), anyhow::Error>(())
        });
        handles.push(handle);
    }

    // Drop the original sender so the channel can close when all clones are dropped
    drop(tx);

    // Collect results from the channel
    let mut filing_map = HashMap::new();
    while let Some((document_path, filing)) = rx.recv().await {
        completed_tasks += 1;
        if !document_path.is_empty() {
            // Only add successful results
            filing_map.insert(document_path, filing);
        }

        // Break if we've received responses for all tasks
        if completed_tasks >= total_tasks {
            break;
        }
    }

    // Wait for all spawned tasks to complete
    for handle in handles {
        if let Err(e) = handle.await {
            log::error!("Task join error: {}", e);
        }
    }

    Ok(filing_map)
}

pub async fn extract_complete_submission_filing(
    filepath: &str,
    store: &dyn langchain_rust::vectorstore::VectorStore,
) -> Result<HashMap<String, serde_json::Value>> {
    log::info!(
        "Starting extract_complete_submission_filing for file: {}",
        filepath
    );

    // Read and decode the file content
    log::debug!("Reading file: {}", filepath);
    let raw_text = fs::read(filepath)?;
    let charenc = detect(&raw_text).0;

    log::debug!("Detected character encoding: {}", charenc);
    let mut reader = DecodeReaderBytesBuilder::new()
        .encoding(Encoding::for_label(charenc.as_bytes()))
        .build(BufReader::new(File::open(filepath)?));

    let mut raw_text_string = String::new();
    reader.read_to_string(&mut raw_text_string)?;

    // Parse XBRL using the xml module
    let facts = super::xbrl::xml::parse_xml_to_facts(raw_text_string);

    // Convert facts to JSON value
    let json_facts = serde_json::to_value(&facts)?;

    // Create filing documents map
    let mut filing_documents = HashMap::new();
    filing_documents.insert("facts".to_string(), json_facts.clone());

    // Create metadata for the document
    let metadata = serde_json::json!({
        "type": "edgar_filing",
        "filepath": filepath,
        "source": "xbrl"
    });

    // Convert the metadata to the required HashMap format
    let metadata_map = metadata.as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    // Create a document with the JSON facts as content
    let doc = langchain_rust::schemas::Document {
        page_content: json_facts.to_string(),
        metadata: metadata_map,
        score: 0.0,
    };

    // Store the document in vector storage
    store.add_documents(&[doc], &langchain_rust::vectorstore::VecStoreOptions::default())
        .await
        .map_err(|e| anyhow!("Failed to store filing in vector storage: {}", e))?;

    log::debug!("filing documents:\n {:?}", filing_documents);
    Ok(filing_documents)
}

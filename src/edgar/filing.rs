use anyhow::{anyhow, Result};
use chardet::detect;
use chrono::NaiveDate;
use encoding_rs::Encoding;
use encoding_rs_io::DecodeReaderBytesBuilder;
use itertools::Itertools;
use langchain_rust::vectorstore::VectorStore;
use log::{error, info};
use mime::{APPLICATION_JSON, TEXT_XML};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use url::Url;

use crate::document::{DocType, Metadata};
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
    pub report_type: ReportType,
    pub file_number: String,
    pub film_number: String,
    pub items: String,
    pub size: i32,
    pub is_xbrl: bool,
    pub is_inline_xbrl: bool,
    pub primary_document: String,
    pub primary_doc_description: String,
}

impl Filing {
    fn matches_report_type(&self, report_types: &[ReportType]) -> bool {
        report_types.iter().any(|rt| &self.report_type == rt)
    }
}

fn process_filing_entries(entry: &FilingEntry, query: &Query) -> Result<Vec<Filing>> {
    let mut filings = Vec::new();

    // Zip all the vectors together and process each record
    for i in 0..entry.accession_number.len() {
        let mrt = ReportType::from_str(&entry.report_type[i][..]);
        if mrt.is_err() {
            return Err(anyhow!(mrt.err().unwrap()));
        }
        let rt = mrt.unwrap();
        let filing = Filing {
            accession_number: entry.accession_number[i].clone(),
            filing_date: entry.filing_date[i],
            report_date: entry.report_date[i].clone(),
            acceptance_date_time: entry.acceptance_date_time[i].clone(),
            act: entry.act[i].clone(),
            report_type: rt,
            file_number: entry.file_number[i].clone(),
            film_number: entry.film_number[i].clone(),
            items: entry.items[i].clone(),
            size: entry.size[i],
            is_xbrl: entry.is_xbrl[i] == 1,
            is_inline_xbrl: entry.is_inline_xbrl[i] == 1,
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

    Ok(filings)
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

async fn fetch_filing_page(
    client: &Client,
    url: &str,
    filepath: &Path,
) -> Result<()> {
    match fetch_and_save(
        client,
        &Url::parse(url)?,
        filepath,
        USER_AGENT,
        APPLICATION_JSON,
        crate::edgar::rate_limiter(),
    )
    .await
    {
        Ok(()) => {
            log::debug!(
                "Successfully fetched and saved {} filing to {}",
                url,
                filepath.to_str().unwrap()
            );
            Ok(())
        }
        Err(e) => {
            // If the file exists despite an error, try to use it anyway
            if !filepath.exists() {
                error!(
                    "Failed to fetch filings from {} and no local file exists: {}",
                    url, e
                );
                return Err(anyhow!("Failed to fetch filings: {}", e));
            }
            log::warn!("Error fetching from {} but local file exists, attempting to use cached version: {}", url, e);
            Ok(())
        }
    }
}

async fn process_filing_page(
    filepath: &Path,
    fetched_count: usize,
    all_filings: &mut Vec<FilingEntry>,
    additional_files: &mut Vec<FilingFile>,
) -> Result<()> {
    let content = fs::read_to_string(filepath).map_err(|e| {
        error!("Failed to read filing file {:?}: {}", filepath, e);
        anyhow!("Failed to read filing file: {}", e)
    })?;

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
        *additional_files = response.filings.files.clone();
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

    Ok(())
}

async fn process_adr_company_filings(
    client: &Client,
    cik: &str,
    limit: Option<usize>,
) -> Result<CompanyFilings> {
    // For ADRs, we use the same processing logic but may want to add special handling in the future
    // TODO: Implement ADR-specific processing if needed:
    // - Different rate limiting
    // - Special handling of certain filing types
    // - Additional metadata processing
    // For now, we use the same logic as regular filings
    get_company_filings_internal(client, cik, limit).await
}

async fn get_company_filings_internal(
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
            fetch_filing_page(client, &current_url, &filepath).await?;
        }

        process_filing_page(&filepath, fetched_count, &mut all_filings, &mut additional_files).await?;

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
    let merged = merge_filing_entries(all_filings);

    // Update the initial response with merged filings
    initial_response.filings.recent = merged.clone();

    log_filing_summary(&merged, &padded_cik);

    Ok(initial_response)
}

fn merge_filing_entries(filings: Vec<FilingEntry>) -> FilingEntry {
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

    for filing in filings {
        merged.accession_number.extend(filing.accession_number);
        merged.filing_date.extend(filing.filing_date);
        merged.report_date.extend(filing.report_date);
        merged.acceptance_date_time.extend(filing.acceptance_date_time);
        merged.act.extend(filing.act);
        merged.report_type.extend(filing.report_type);
        merged.file_number.extend(filing.file_number);
        merged.film_number.extend(filing.film_number);
        merged.items.extend(filing.items);
        merged.size.extend(filing.size);
        merged.is_xbrl.extend(filing.is_xbrl);
        merged.is_inline_xbrl.extend(filing.is_inline_xbrl);
        merged.primary_document.extend(filing.primary_document);
        merged.primary_doc_description.extend(filing.primary_doc_description);
    }

    merged
}

fn log_filing_summary(merged: &FilingEntry, padded_cik: &str) {
    let unique_report_types: std::collections::HashSet<_> = merged.report_type.iter().collect();
    info!(
        "Fetched filings summary for CIK {}: {} total filings, {} unique report types ({}), date range: {} to {}",
        padded_cik,
        merged.accession_number.len(),
        unique_report_types.len(),
        unique_report_types.into_iter().join(", "),
        merged.filing_date.iter().min().map_or("N/A".to_string(), |d| d.to_string()),
        merged.filing_date.iter().max().map_or("N/A".to_string(), |d| d.to_string())
    );
}

pub async fn get_company_filings(
    client: &Client,
    cik: &str,
    limit: Option<usize>,
    is_adr: bool,
) -> Result<CompanyFilings> {
    if is_adr {
        process_adr_company_filings(client, cik, limit).await
    } else {
        get_company_filings_internal(client, cik, limit).await
    }
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

    // Fetch filings using the CIK and ADR status from query
    let filings = get_company_filings(client, cik, None, query.is_adr).await?;
    let matching_filings = process_filing_entries(&filings.filings.recent, query)?;

    crate::utils::dirs::ensure_edgar_dirs()?;

    // Create a bounded channel
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    // Keep track of how many tasks we spawn
    let total_tasks = matching_filings.len();
    let mut completed_tasks = 0;

    info!(
        "Found {} matching filings for query parameters:\n\
         - Report types: {}\n\
         - Date range: {} to {}",
        matching_filings.len(),
        matching_filings
            .iter()
            .map(|f| f.report_type.to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", "),
        query.start_date,
        query.end_date
    );

    // Spawn async tasks to fetch and save each matching filing
    let mut handles = Vec::new();
    for filing in matching_filings {
        let tx = tx.clone();
        let client = client.clone();
        let cik = cik.to_string();
        let handle = tokio::spawn(async move {
            let filing_clone = filing.clone();
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
    report_type: ReportType,
    store: &dyn VectorStore,
    pg_pool: &Pool<Postgres>,
) -> Result<()> {
    log::info!(
        "Starting extract_complete_submission_filing for file: {}",
        filepath
    );
    log::info!("Parsing XBRL file");

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
    let facts = super::xbrl::parse_xml_to_facts(raw_text_string);

    // Generate markdown
    let xbrl_filing = super::xbrl::XBRLFiling {
        raw_facts: Some(facts.clone()),
        fact_table: None,
        dimensions: None,
    };

    let markdown_content = xbrl_filing.to_markdown();
    if log::log_enabled!(log::Level::Debug) {
        log::debug!("Generated markdown content:\n{}", markdown_content);
    }

    // Extract CIK and accession number from filepath
    let path = Path::new(filepath);
    let parts: Vec<&str> = path
        .parent()
        .unwrap()
        .to_str()
        .unwrap()
        .split('/')
        .collect();
    let cik = parts[parts.len() - 2];
    let accession_number = parts[parts.len() - 1];

    // Create markdown directory
    let markdown_dir = format!("data/edgar/parsed/{}/{}", cik, accession_number);
    fs::create_dir_all(&markdown_dir)?;

    // Save markdown file
    let markdown_path = format!("{}/filing.md", markdown_dir);
    fs::write(&markdown_path, &markdown_content)?;
    log::info!("Saved markdown version to: {}", markdown_path);

    let symbol = crate::edgar::tickers::get_ticker_for_cik(cik).await?;

    // Create metadata for the document
    log::debug!("Creating metadata with report_type: {}", report_type);
    let metadata = Metadata::MetaEdgarFiling {
        doc_type: DocType::EdgarFiling,
        filepath: filepath.into(),
        filing_type: report_type,
        cik: cik.to_string(),
        accession_number: accession_number.to_string(),
        symbol,
        chunk_index: 0,  // Set appropriately
        total_chunks: 1, // Set appropriately
    };

    // Save metadata alongside markdown
    let metadata_path = format!("{}/filing.json", markdown_dir);
    let hashmap: HashMap<String, Value> = metadata.clone().into();
    fs::write(&metadata_path, serde_json::to_string_pretty(&hashmap)?)?;
    log::info!("Saved metadata to: {}", metadata_path);

    // Store the markdown content using the chunking utility with caching
    crate::document::store_chunked_document(markdown_content, metadata, store, pg_pool).await?;

    log::info!("Added filing document to vector store: {}", filepath);

    log::debug!("Filing processed and converted to markdown");
    Ok(())
}

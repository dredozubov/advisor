use std::path::{Path, PathBuf};
use url::Url;
use std::collections::HashMap;
use anyhow::Result;
use reqwest::Client;

use crate::edgar::utils::fetch_and_save;

// Hardcoded values
const FILING_DATA_DIR: &str = "/path/to/filing/data";
const EDGAR_ARCHIVES_URL: &str = "https://www.sec.gov/Archives/";
const USER_AGENT: &str = "YourCompanyName YourAppName YourEmail";

pub fn generate_filepaths(sec_filing: &HashMap<String, String>) -> HashMap<String, String> {
    let mut feed_item = sec_filing.clone();
    
    let cik_directory = FILING_DATA_DIR
        .replace("CIK", &feed_item["CIK"])
        .replace("FOLDER", "");
    feed_item.insert("cik_directory".to_string(), cik_directory.clone());

    let filename = Path::new(&feed_item["Filename"]);
    let filing_filepath = Path::new(&cik_directory).join(filename.file_name().unwrap());
    feed_item.insert("filing_filepath".to_string(), filing_filepath.to_str().unwrap().to_string());

    let filing_zip_filepath = filing_filepath.with_extension("zip");
    feed_item.insert("filing_zip_filepath".to_string(), filing_zip_filepath.to_str().unwrap().to_string());

    let filing_folder = filename.file_stem().unwrap().to_str().unwrap().replace("-", "");
    feed_item.insert("filing_folder".to_string(), filing_folder.clone());

    let extracted_filing_directory = FILING_DATA_DIR
        .replace("CIK", &feed_item["CIK"])
        .replace("FOLDER", &filing_folder);
    feed_item.insert("extracted_filing_directory".to_string(), extracted_filing_directory);

    let edgar_archives_url = Url::parse(EDGAR_ARCHIVES_URL).unwrap();
    let filing_url = edgar_archives_url.join(&feed_item["Filename"]).unwrap().to_string();
    feed_item.insert("filing_url".to_string(), filing_url);

    feed_item
}

pub async fn process(client: &Client, filing_meta: &HashMap<String, String>) -> Result<String> {
    let filing_filepaths = generate_filepaths(filing_meta);
    
    let filing_url = Url::parse(&filing_filepaths["filing_url"])?;
    let filepath = Path::new(&filing_filepaths["filing_filepath"]);
    
    fetch_and_save(client, &filing_url, filepath, USER_AGENT).await?;
    
    let filing_content = extract(filing_filepaths)?;
    
    Ok(filing_content)
}

fn extract(filing_filepaths: HashMap<String, String>) -> Result<String> {
    // Implement extraction logic here
    Ok("Extracted content".to_string())
}

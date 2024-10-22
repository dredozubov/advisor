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
    let mut feed_item = sec_filing.clone();

    let cik_directory = FILING_DATA_DIR
        .replace("CIK", &feed_item["CIK"])
        .replace("FOLDER", "");
    feed_item.insert("cik_directory".to_string(), cik_directory.clone());

    let filename = Path::new(&sec_filing["Filename"]);
    let filing_filepath = Path::new(&cik_directory).join(filename.file_name().unwrap());
    let filing_zip_filepath = filing_filepath.with_extension("zip");
    let filing_folder = filename
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .replace("-", "");

    feed_item.insert(
        "filing_filepath".to_string(),
        filing_filepath.to_str().unwrap().to_string(),
    );
    feed_item.insert(
        "filing_zip_filepath".to_string(),
        filing_zip_filepath.to_str().unwrap().to_string(),
    );
    feed_item.insert("filing_folder".to_string(), filing_folder.clone());

    let extracted_filing_directory = FILING_DATA_DIR
        .replace("CIK", &feed_item["CIK"])
        .replace("FOLDER", &filing_folder);
    feed_item.insert(
        "extracted_filing_directory".to_string(),
        extracted_filing_directory,
    );

    let edgar_archives_url = Url::parse(EDGAR_ARCHIVES_URL).unwrap();
    let filing_url = edgar_archives_url
        .join(&sec_filing["Filename"])
        .unwrap()
        .to_string();
    feed_item.insert("filing_url".to_string(), filing_url);

    feed_item
}

pub async fn process_filing(
    client: &Client,
    filing_meta: &[(&str, &str)],
) -> Result<Filing> {
    let filing_filepaths = generate_filepaths(&filing_meta.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect());

    let filing_url = Url::parse(&filing_filepaths["filing_url"])?;
    let filepath = Path::new(&filing_filepaths["filing_filepath"]);

    super::utils::fetch_and_save(client, &filing_url, filepath, USER_AGENT).await?;

    let filing_content = extract(&filing_filepaths)?;

    // Combine all document contents into a single string
    let content = filing_content
        .values()
        .filter_map(|v| v.as_object())
        .filter_map(|obj| obj.get("content"))
        .filter_map(|v| v.as_str())
        .collect::<Vec<&str>>()
        .join("\n\n");

    // Extract metadata
    let metadata = filing_content
        .values()
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
    let mut filing_contents = HashMap::new();
    let extracted_filing_directory = Path::new(&filing_json["extracted_filing_directory"]);
    let extracted_filing_directory_zip = extracted_filing_directory.with_extension("zip");

    if !extracted_filing_directory.exists() && !extracted_filing_directory_zip.exists() {
        info!("\n\n\n\n\tExtracting Filing Documents:\n");

        match extract_complete_submission_filing(
            &filing_json["filing_filepath"],
            Some(extracted_filing_directory),
        ) {
            Ok(contents) => {
                filing_contents = contents;
            }
            Err(e) => {
                error!("\n\n\n\nError Decoding \n\n{}", e);
            }
        }

        info!("\n\n\n\n\tExtraction Complete\n");
    }

    Ok(filing_contents)
}

pub fn extract_complete_submission_filing(
    filepath: &str,
    output_directory: Option<&Path>,
) -> Result<HashMap<String, serde_json::Value>> {
    let elements_list = vec![
        ("FILENAME", "<FILENAME>"),
        ("TYPE", "<TYPE>"),
        ("SEQUENCE", "<SEQUENCE>"),
        ("DESCRIPTION", "<DESCRIPTION>"),
    ];

    let output_directory = output_directory.unwrap_or_else(|| Path::new(""));
    if !output_directory.exists() {
        fs::create_dir_all(output_directory)?;
    }

    info!("extracting documents to {:?}", output_directory);

    let xbrl_doc = Regex::new(r"<DOCUMENT>(.*?)</DOCUMENT>")?;
    let xbrl_text = Regex::new(r"<(TEXT|text)>(.*?)</(TEXT|text)>")?;

    let raw_text = fs::read(filepath)?;
    let charenc = detect(&raw_text).0;

    let mut raw_text = DecodeReaderBytesBuilder::new()
        .encoding(Encoding::for_label(charenc.as_bytes()))
        .build(BufReader::new(File::open(filepath)?));

    let mut raw_text_string = String::new();
    raw_text.read_to_string(&mut raw_text_string)?;

    let filing_header = header_parser(&raw_text_string)?;

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

        let doc_num = format!("{:04}", i + 1);
        let output_filename = format!(
            "{}-({}) {} {}",
            doc_num,
            filing_document.get("TYPE").unwrap_or(&"".to_string()),
            filing_document
                .get("DESCRIPTION")
                .unwrap_or(&"".to_string()),
            filing_document.get("FILENAME").unwrap_or(&"".to_string())
        );
        let output_filename = format_filename(&output_filename);
        let output_filepath = output_directory.join(&output_filename);

        if raw_text.to_lowercase().starts_with("begin")
            || document.to_lowercase().starts_with("begin")
        {
            // Handle UUEncoded content
            let decoded_content = uudecode(&raw_text)
                .ok_or_else(|| anyhow::anyhow!("Failed to decode UUEncoded content"))?;
            let mut file = File::create(&output_filepath)?;
            file.write_all(&decoded_content.0)?; // Write the Vec<u8> part
        } else {
            fs::write(&output_filepath, raw_text)?;
        }

        filing_document.insert(
            "RELATIVE_FILEPATH".to_string(),
            output_filepath.to_str().unwrap().to_string(),
        );
        filing_document.insert("DESCRIPTIVE_FILEPATH".to_string(), output_filename);
        filing_document.insert("FILE_SIZE".to_string(), file_size(&output_filepath)?);
        filing_document.insert(
            "FILE_SIZE_BYTES".to_string(),
            fs::metadata(&output_filepath)?.len().to_string(),
        );

        filing_documents.insert(i.to_string(), json!(filing_document));
    }

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

use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::fs;

use std::collections::HashMap;

const ELEMENTS_LIST: &[(&str, &str)] = &[
    ("FILENAME", "<FILENAME>"),
    ("TYPE", "<TYPE>"),
    ("SEQUENCE", "<SEQUENCE>"),
    ("DESCRIPTION", "<DESCRIPTION>"),
];

pub fn parse_documents(raw_text: &str, output_directory: &Path) -> Result<HashMap<String, serde_json::Value>> {
    let xbrl_doc = Regex::new(r"<DOCUMENT>(.*?)</DOCUMENT>")?;
    let xbrl_text = Regex::new(r"<(TEXT|text)>(.*?)</(TEXT|text)>")?;
    let mut filing_documents = HashMap::new();

    let documents: Vec<_> = xbrl_doc
        .find_iter(raw_text)
        .map(|m| m.as_str())
        .collect();

    for (i, document) in documents.iter().enumerate() {
        let mut filing_document = HashMap::new();

        // Extract document information
        for (element, element_path) in ELEMENTS_LIST {
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

        if let Err(e) = fs::write(&output_filepath, &raw_text) {
            log::error!(
                "Failed to write parsed content to file: {:?}. Error: {}",
                output_filepath,
                e
            );
            return Err(e.into());
        }

        filing_document.insert(
            "RELATIVE_FILEPATH".to_string(),
            output_filepath.to_str().unwrap().to_string(),
        );
        filing_document.insert("DESCRIPTIVE_FILEPATH".to_string(), output_filename);
        filing_document.insert(
            "FILE_SIZE".to_string(),
            format!("{:.2} KB", fs::metadata(&output_filepath)?.len() as f64 / 1024.0),
        );
        filing_document.insert(
            "FILE_SIZE_BYTES".to_string(),
            fs::metadata(&output_filepath)?.len().to_string(),
        );

        filing_documents.insert(i.to_string(), serde_json::json!(filing_document));
    }

    Ok(filing_documents)
}
pub fn header_parser(raw_text: &str) -> Result<Vec<(String, String)>> {
    let mut headers = Vec::new();
    let re = Regex::new(r"<HEADER>(.*?)</HEADER>")?;
    
    if let Some(captures) = re.captures(raw_text) {
        if let Some(header_content) = captures.get(1) {
            for line in header_content.as_str().lines() {
                let line = line.trim();
                if let Some(colon_idx) = line.find(':') {
                    let key = line[..colon_idx].trim().to_string();
                    let value = line[colon_idx + 1..].trim().to_string();
                    headers.push((key, value));
                }
            }
        }
    }
    
    Ok(headers)
}

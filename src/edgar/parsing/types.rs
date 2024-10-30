use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct FilingDocument {
    pub filename: Option<String>,
    pub doc_type: Option<String>,
    pub sequence: Option<String>,
    pub description: Option<String>,
    pub relative_filepath: Option<String>,
    pub descriptive_filepath: Option<String>,
    pub file_size: Option<String>,
    pub file_size_bytes: Option<String>,
    pub raw_text: String,
}

pub type FilingDocuments = HashMap<String, serde_json::Value>;

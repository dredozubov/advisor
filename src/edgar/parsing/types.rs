use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SectionType {
    ManagementDiscussion,
    FinancialStatements,
    Notes,
    RiskFactors,
    BusinessDescription,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilingSection {
    pub section_type: SectionType,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Period {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub instant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilingFact {
    pub context: String,
    pub value: String,
    pub unit: Option<String>,
    pub period: Period,
    pub formatted_value: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilingDocument {
    pub sections: Vec<FilingSection>,
    pub facts: Vec<FilingFact>,
    pub path: PathBuf,
}

use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "String")]
pub enum ReportType {
    Form10K,
    Form10Q,
    Form8K,
    Form4,
    Form5,
    FormS1,
    FormS3,
    FormS4,
    FormDEF14A,
    Form13F,
    Form13G,
    Form13D,
    FormSD,
    Form6K,
    Form20F,
    FormN1A,
    FormNCSR,
    FormNPORT,
    FormNQ,
    Other(String),
}

impl ReportType {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "10-K" => ReportType::Form10K,
            "10-Q" => ReportType::Form10Q,
            "8-K" => ReportType::Form8K,
            "4" => ReportType::Form4,
            "5" => ReportType::Form5,
            "S-1" => ReportType::FormS1,
            "S-3" => ReportType::FormS3,
            "S-4" => ReportType::FormS4,
            "DEF 14A" => ReportType::FormDEF14A,
            "13F" => ReportType::Form13F,
            "13G" => ReportType::Form13G,
            "13D" => ReportType::Form13D,
            "SD" => ReportType::FormSD,
            "6-K" => ReportType::Form6K,
            "20-F" => ReportType::Form20F,
            "N-1A" => ReportType::FormN1A,
            "N-CSR" => ReportType::FormNCSR,
            "N-PORT" => ReportType::FormNPORT,
            "N-Q" => ReportType::FormNQ,
            _ => ReportType::Other(s.to_string()),
        }
    }
}

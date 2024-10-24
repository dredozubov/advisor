use once_cell::sync::Lazy;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "String")]
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

impl TryFrom<String> for ReportType {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Ok(ReportType::from_str(&s))
    }
}

pub static REPORT_TYPES: Lazy<String> = Lazy::new(|| {
    let types = vec![
        "10-K", "10-Q", "8-K", "4", "5", "S-1", "S-3", "S-4",
        "DEF 14A", "13F", "13G", "13D", "SD", "6-K", "20-F",
        "N-1A", "N-CSR", "N-PORT", "N-Q"
    ];
    types.join(", ")
});

impl ReportType {
    pub fn list_types() -> &'static str {
        &REPORT_TYPES
    }

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

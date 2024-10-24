use once_cell::sync::Lazy;
use serde::Deserialize;
use std::fmt;

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

impl fmt::Display for ReportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReportType::Form10K => write!(f, "10-K"),
            ReportType::Form10Q => write!(f, "10-Q"),
            ReportType::Form8K => write!(f, "8-K"),
            ReportType::Form4 => write!(f, "4"),
            ReportType::Form5 => write!(f, "5"),
            ReportType::FormS1 => write!(f, "S-1"),
            ReportType::FormS3 => write!(f, "S-3"),
            ReportType::FormS4 => write!(f, "S-4"),
            ReportType::FormDEF14A => write!(f, "DEF 14A"),
            ReportType::Form13F => write!(f, "13F"),
            ReportType::Form13G => write!(f, "13G"),
            ReportType::Form13D => write!(f, "13D"),
            ReportType::FormSD => write!(f, "SD"),
            ReportType::Form6K => write!(f, "6-K"),
            ReportType::Form20F => write!(f, "20-F"),
            ReportType::FormN1A => write!(f, "N-1A"),
            ReportType::FormNCSR => write!(f, "N-CSR"),
            ReportType::FormNPORT => write!(f, "N-PORT"),
            ReportType::FormNQ => write!(f, "N-Q"),
            ReportType::Other(s) => write!(f, "{}", s),
        }
    }
}

pub static REPORT_TYPES: Lazy<String> = Lazy::new(|| {
    let types = vec![
        ReportType::Form10K,
        ReportType::Form10Q,
        ReportType::Form8K,
        ReportType::Form4,
        ReportType::Form5,
        ReportType::FormS1,
        ReportType::FormS3,
        ReportType::FormS4,
        ReportType::FormDEF14A,
        ReportType::Form13F,
        ReportType::Form13G,
        ReportType::Form13D,
        ReportType::FormSD,
        ReportType::Form6K,
        ReportType::Form20F,
        ReportType::FormN1A,
        ReportType::FormNCSR,
        ReportType::FormNPORT,
        ReportType::FormNQ,
    ];
    types.iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ")
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

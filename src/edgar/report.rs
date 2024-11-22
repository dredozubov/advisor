use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use strum::{EnumIter, IntoEnumIterator};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, EnumIter)]
#[serde(try_from = "String")]
pub enum ReportType {
    Form10K,
    Form6K,
    Form10Q,
    Form20K,
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
    Form20F,
    FormN1A,
    FormNCSR,
    FormNPORT,
    FormNQ,
    Form144,
    Other(String),
}

impl TryFrom<String> for ReportType {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        ReportType::from_str(&s)
    }
}

impl fmt::Display for ReportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReportType::Form10K => write!(f, "10-K"),
            ReportType::Form6K => write!(f, "6-K"),
            ReportType::Form10Q => write!(f, "10-Q"),
            ReportType::Form20K => write!(f, "20-K"),
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
            ReportType::Form20F => write!(f, "20-F"),
            ReportType::FormN1A => write!(f, "N-1A"),
            ReportType::FormNCSR => write!(f, "N-CSR"),
            ReportType::FormNPORT => write!(f, "N-PORT"),
            ReportType::FormNQ => write!(f, "N-Q"),
            ReportType::Form144 => write!(f, "144"),
            ReportType::Other(s) => write!(f, "{}", s),
        }
    }
}

pub static REPORT_TYPES: Lazy<String> = Lazy::new(|| {
    ReportType::iter()
        .filter(|t| !matches!(t, ReportType::Other(_)))
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ")
});

impl ReportType {
    pub fn list_types() -> &'static str {
        &REPORT_TYPES
    }
}

impl FromStr for ReportType {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<ReportType, std::string::String> {
        match s.to_uppercase().as_str() {
            "10-K" => Ok(ReportType::Form10K),
            "6-K" => Ok(ReportType::Form6K),
            "10-Q" => Ok(ReportType::Form10Q),
            "20-Q" => Ok(ReportType::Form20K),
            "8-K" => Ok(ReportType::Form8K),
            "4" => Ok(ReportType::Form4),
            "5" => Ok(ReportType::Form5),
            "S-1" => Ok(ReportType::FormS1),
            "S-3" => Ok(ReportType::FormS3),
            "S-4" => Ok(ReportType::FormS4),
            "DEF 14A" => Ok(ReportType::FormDEF14A),
            "13F" => Ok(ReportType::Form13F),
            "13G" => Ok(ReportType::Form13G),
            "13D" => Ok(ReportType::Form13D),
            "SD" => Ok(ReportType::FormSD),
            "20-F" => Ok(ReportType::Form20F),
            "N-1A" => Ok(ReportType::FormN1A),
            "N-CSR" => Ok(ReportType::FormNCSR),
            "N-PORT" => Ok(ReportType::FormNPORT),
            "N-Q" => Ok(ReportType::FormNQ),
            "144" => Ok(ReportType::Form144),
            _ => Ok(ReportType::Other(s.to_string())),
        }
    }
}

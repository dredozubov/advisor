use anyhow::{anyhow, Result};
use crate::edgar;
use crate::earnings;
use chrono::NaiveDate;
use serde::{self, Deserialize, Serialize};

/// A high-level query type that can handle multiple data sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    /// List of stock tickers to query
    pub tickers: Vec<String>,
    /// Parameters for different data sources
    pub parameters: serde_json::Value,
}

impl Query {
    pub fn new(tickers: Vec<String>) -> Self {
        Query {
            tickers,
            parameters: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    pub fn with_edgar_query(mut self, params: serde_json::Value) -> Self {
        if let serde_json::Value::Object(ref mut map) = self.parameters {
            map.insert("filings".to_string(), params);
        }
        self
    }

    pub fn with_earnings_query(mut self, params: serde_json::Value) -> Self {
        if let serde_json::Value::Object(ref mut map) = self.parameters {
            map.insert("earnings".to_string(), params);
        }
        self
    }

    pub fn validate(&self) -> Result<()> {
        if self.tickers.is_empty() {
            return Err(anyhow!("At least one ticker must be specified"));
        }
        if self.start_date > self.end_date {
            return Err(anyhow!("Start date must be before or equal to end date"));
        }
        if self.edgar_query.is_none() && self.earnings_query.is_none() {
            return Err(anyhow!("At least one query type (EDGAR or earnings) must be specified"));
        }
        Ok(())
    }

    pub fn has_edgar_query(&self) -> bool {
        self.edgar_query.is_some()
    }

    pub fn has_earnings_query(&self) -> bool {
        self.earnings_query.is_some()
    }
}

pub mod date_format {
    use chrono::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};
    const FORMAT: &str = "%Y-%m-%d";

    pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.format("%Y-%m-%d").to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(serde::de::Error::custom)
    }
}

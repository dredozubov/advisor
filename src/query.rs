use anyhow::{anyhow, Result};
use serde::{self, Deserialize, Serialize};
use serde_json::Value;
use crate::edgar::{self, query as edgar_query, report};
use crate::earnings;

/// A high-level query type that can handle multiple data sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    /// List of stock tickers to query
    pub tickers: Vec<String>,
    /// Parameters for different data sources
    pub parameters: Value,
}

impl Query {
    pub fn new(tickers: Vec<String>) -> Self {
        Query {
            tickers,
            parameters: Value::Object(serde_json::Map::new()),
        }
    }

    pub fn with_edgar_query(mut self, params: Value) -> Self {
        if let Value::Object(ref mut map) = self.parameters {
            map.insert("filings".to_string(), params);
        }
        self
    }

    pub fn with_earnings_query(mut self, params: Value) -> Self {
        if let Value::Object(ref mut map) = self.parameters {
            map.insert("earnings".to_string(), params);
        }
        self
    }

    pub fn validate(&self) -> Result<()> {
        if self.tickers.is_empty() {
            return Err(anyhow!("At least one ticker must be specified"));
        }
        
        let has_filings = self.parameters.get("filings").is_some();
        let has_earnings = self.parameters.get("earnings").is_some();
        
        if !has_filings && !has_earnings {
            return Err(anyhow!("At least one query type (filings or earnings) must be specified"));
        }
        Ok(())
    }

    pub fn has_edgar_query(&self) -> bool {
        self.parameters.get("filings").is_some()
    }

    pub fn has_earnings_query(&self) -> bool {
        self.parameters.get("earnings").is_some()
    }

    pub fn to_edgar_query(&self) -> Result<edgar_query::Query> {
        if let Some(filings) = self.parameters.get("filings") {
            let start_date = filings.get("start_date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("start_date missing or invalid"))?;
            let end_date = filings.get("end_date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("end_date missing or invalid"))?;
            let report_types = filings.get("report_types")
                .and_then(|v| v.as_array())
                .ok_or_else(|| anyhow!("report_types missing or invalid"))?;

            let start = chrono::NaiveDate::parse_from_str(start_date, "%Y-%m-%d")?;
            let end = chrono::NaiveDate::parse_from_str(end_date, "%Y-%m-%d")?;
            
            let types: Result<Vec<report::ReportType>> = report_types
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.parse())
                .collect();

            Ok(edgar_query::Query::new(
                self.tickers.clone(),
                start,
                end,
                types?,
            )?)
        } else {
            Err(anyhow!("No filings parameters found"))
        }
    }

    pub fn to_earnings_query(&self) -> Result<earnings::Query> {
        if let Some(earnings) = self.parameters.get("earnings") {
            let start_date = earnings.get("start_date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("start_date missing or invalid"))?;
            let end_date = earnings.get("end_date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("end_date missing or invalid"))?;

            let start = chrono::NaiveDate::parse_from_str(start_date, "%Y-%m-%d")?;
            let end = chrono::NaiveDate::parse_from_str(end_date, "%Y-%m-%d")?;

            Ok(earnings::Query {
                ticker: self.tickers[0].clone(), // Use first ticker for earnings
                start_date: start,
                end_date: end,
            })
        } else {
            Err(anyhow!("No earnings parameters found"))
        }
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

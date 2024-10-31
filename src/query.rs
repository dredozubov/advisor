use anyhow::{anyhow, Result};
use crate::edgar;
use crate::earnings;
use chrono::NaiveDate;
use serde::{self, Deserialize, Serialize, Serializer, Deserializer};

/// A high-level query type that can handle multiple data sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    /// List of stock tickers to query
    pub tickers: Vec<String>,
    /// Start date for the query period
    #[serde(with = "date_format")]
    pub start_date: NaiveDate,
    /// End date for the query period
    #[serde(with = "date_format")]
    pub end_date: NaiveDate,
    /// Optional EDGAR query parameters
    pub edgar_query: Option<edgar::query::Query>,
    /// Optional earnings query parameters
    pub earnings_query: Option<earnings::Query>,
}

impl Query {
    pub fn new(
        tickers: Vec<String>,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Self {
        Query {
            tickers,
            start_date,
            end_date,
            edgar_query: None,
            earnings_query: None,
        }
    }

    pub fn with_edgar_query(mut self, report_types: Vec<edgar::report::ReportType>) -> Self {
        self.edgar_query = Some(edgar::query::Query {
            tickers: self.tickers.clone(),
            start_date: self.start_date,
            end_date: self.end_date,
            report_types,
        });
        self
    }

    pub fn with_earnings_query(mut self) -> Self {
        self.earnings_query = Some(earnings::Query {
            ticker: self.tickers[0].clone(), // Assuming first ticker for earnings
            start_date: self.start_date,
            end_date: self.end_date,
        });
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

mod date_format {
    use chrono::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

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

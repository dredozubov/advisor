use crate::edgar::report::ReportType;
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::report;

/// Query parameters for fetching SEC EDGAR filings
/// 
/// # JSON Format
/// The deserializer accepts JSON in this format:
/// ```json
/// {
///   "tickers": ["AAPL", "GOOGL"],
///   "start_date": "2024-01-01", 
///   "end_date": "2024-12-31",
///   "report_types": ["10-K", "10-Q", "8-K"]
/// }
/// ```
/// 
/// Example with all supported report types:
/// ```json
/// {
///   "tickers": ["TSLA"],
///   "start_date": "2023-01-01",
///   "end_date": "2023-12-31", 
///   "report_types": [
///     "10-K",    // Annual report
///     "10-Q",    // Quarterly report  
///     "8-K",     // Current report
///     "4",       // Changes in ownership
///     "5",       // Annual ownership changes
///     "S-1",     // IPO registration
///     "S-3",     // Simplified registration
///     "S-4",     // Merger/acquisition
///     "DEF 14A"  // Proxy statement
///   ]
/// }
/// ```
///
/// Example JSON that can be deserialized:
/// ```rust
/// use advisor::edgar::query::Query;
/// let json_str = r#"{
///   "tickers": ["AAPL"],
///   "start_date": "2024-01-01",
///   "end_date": "2024-12-31",
///   "report_types": ["10-K", "10-Q"]
/// }"#;
/// let query = Query::from_json(json_str).unwrap();
/// ```
/// 
/// Fields:
/// - `tickers`: Array of company stock ticker symbols (strings)
/// - `start_date`: Start date in YYYY-MM-DD format
/// - `end_date`: End date in YYYY-MM-DD format  
/// - `report_types`: Array of SEC filing types to fetch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub tickers: Vec<String>,
    #[serde(with = "date_format")]
    pub start_date: NaiveDate,
    #[serde(with = "date_format")]
    pub end_date: NaiveDate,
    pub report_types: Vec<ReportType>,
}

pub mod date_format {
    use chrono::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d";

    pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.format(FORMAT).to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NaiveDate::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
    }
}

impl Query {
    pub fn new(
        tickers: Vec<String>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        report_types: Vec<report::ReportType>,
    ) -> Result<Self> {
        let query = Query {
            tickers,
            start_date,
            end_date,
            report_types,
        };
        query.validate()?;
        Ok(query)
    }

    pub fn builder() -> QueryBuilder {
        QueryBuilder::default()
    }

    fn validate(&self) -> Result<()> {
        if self.tickers.is_empty() {
            return Err(anyhow!("At least one ticker must be specified"));
        }
        if self.report_types.is_empty() {
            return Err(anyhow!("At least one report type must be specified"));
        }
        if self.start_date > self.end_date {
            return Err(anyhow!("Start date must be before or equal to end date"));
        }
        Ok(())
    }

    /// Parse a JSON string into a Query object
    ///
    /// The JSON format should be as follows:
    /// {
    ///   "tickers": ["AAPL", "GOOGL"],
    ///   "start_date": "2024-01-01",
    ///   "end_date": "2024-12-31",
    ///   "report_types": ["10-K", "10-Q"]
    /// }
    ///
    /// - tickers: An array of stock ticker symbols (strings)
    /// - start_date: The start date for the query in YYYY-MM-DD format
    /// - end_date: The end date for the query in YYYY-MM-DD format
    /// - report_types: An array of report types (strings)
    ///
    /// Valid report types are: "10-K", "10-Q", "8-K", "4", "5", "S-1", "S-3", "S-4",
    /// "DEF 14A", "13F", "13G", "13D", "SD", "6-K", "20-F", "N-1A", "N-CSR", "N-PORT", "N-Q"
    pub fn from_json(json_str: &str) -> Result<Self> {
        serde_json::from_str(json_str).map_err(|e| anyhow!("Failed to parse JSON: {}", e))
    }
}

#[derive(Default)]
pub struct QueryBuilder {
    tickers: Option<Vec<String>>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    report_types: Option<Vec<report::ReportType>>,
}

impl QueryBuilder {
    pub fn tickers(mut self, tickers: Vec<String>) -> Self {
        self.tickers = Some(tickers);
        self
    }

    pub fn start_date(mut self, start_date: NaiveDate) -> Self {
        self.start_date = Some(start_date);
        self
    }

    pub fn end_date(mut self, end_date: NaiveDate) -> Self {
        self.end_date = Some(end_date);
        self
    }

    pub fn report_types(mut self, report_types: Vec<report::ReportType>) -> Self {
        self.report_types = Some(report_types);
        self
    }

    pub fn build(self) -> Result<Query> {
        let tickers = self.tickers.ok_or_else(|| anyhow!("Tickers must be specified"))?;
        let start_date = self.start_date.ok_or_else(|| anyhow!("Start date must be specified"))?;
        let end_date = self.end_date.ok_or_else(|| anyhow!("End date must be specified"))?;
        let report_types = self.report_types.ok_or_else(|| anyhow!("Report types must be specified"))?;

        Query::new(tickers, start_date, end_date, report_types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_from_json() {
        let json_str = r#"
        {
            "tickers": ["AAPL", "GOOGL"],
            "start_date": "2024-01-01",
            "end_date": "2024-12-31",
            "report_types": ["10-K", "10-Q"]
        }
        "#;

        let query = Query::from_json(json_str).unwrap();

        assert_eq!(query.tickers, vec!["AAPL", "GOOGL"]);
        assert_eq!(
            query.start_date,
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
        );
        assert_eq!(
            query.end_date,
            NaiveDate::from_ymd_opt(2024, 12, 31).unwrap()
        );
        assert_eq!(
            query.report_types,
            vec![report::ReportType::Form10K, report::ReportType::Form10Q]
        );
    }

    #[test]
    fn test_query_builder() {
        let query = Query::builder()
            .tickers(vec!["AAPL".to_string()])
            .start_date(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
            .end_date(NaiveDate::from_ymd_opt(2024, 12, 31).unwrap())
            .report_types(vec![report::ReportType::Form10K])
            .build()
            .unwrap();

        assert_eq!(query.tickers, vec!["AAPL"]);
        assert_eq!(query.report_types, vec![report::ReportType::Form10K]);
    }

    #[test]
    fn test_query_validation() {
        // Test empty tickers
        let result = Query::builder()
            .tickers(vec![])
            .start_date(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
            .end_date(NaiveDate::from_ymd_opt(2024, 12, 31).unwrap())
            .report_types(vec![report::ReportType::Form10K])
            .build();
        assert!(result.is_err());

        // Test invalid date range
        let result = Query::builder()
            .tickers(vec!["AAPL".to_string()])
            .start_date(NaiveDate::from_ymd_opt(2024, 12, 31).unwrap())
            .end_date(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
            .report_types(vec![report::ReportType::Form10K])
            .build();
        assert!(result.is_err());
    }
}

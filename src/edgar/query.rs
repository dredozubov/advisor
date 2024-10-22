use crate::edgar::report;
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use serde::{Deserialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct Query {
    pub tickers: Vec<String>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub start_date: NaiveDate,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub end_date: NaiveDate,
    pub report_types: Vec<report::ReportType>,
}

impl Query {
    pub fn new(
        tickers: Vec<String>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        report_types: Vec<report::ReportType>,
    ) -> Self {
        Query {
            tickers,
            start_date,
            end_date,
            report_types,
        }
    }

    /// Parse a JSON string into a Query object
    ///
    /// The JSON format should be as follows:
    /// {
    ///   "tickers": ["AAPL", "GOOGL"],
    ///   "start_date": "2023-01-01",
    ///   "end_date": "2023-12-31",
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
        let v: Value = serde_json::from_str(json_str)?;

        let tickers = v["tickers"]
            .as_array()
            .ok_or_else(|| anyhow!("tickers must be an array"))?
            .iter()
            .map(|t| t.as_str().unwrap_or_default().to_string())
            .collect();

        let start_date = NaiveDate::parse_from_str(
            v["start_date"]
                .as_str()
                .ok_or_else(|| anyhow!("start_date must be a string"))?,
            "%Y-%m-%d",
        )?;

        let end_date = NaiveDate::parse_from_str(
            v["end_date"]
                .as_str()
                .ok_or_else(|| anyhow!("end_date must be a string"))?,
            "%Y-%m-%d",
        )?;

        let report_types = v["report_types"]
            .as_array()
            .ok_or_else(|| anyhow!("report_types must be an array"))?
            .iter()
            .map(|t| report::ReportType::from_str(t.as_str().unwrap_or_default()))
            .collect();

        Ok(Query::new(tickers, start_date, end_date, report_types))
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
            "start_date": "2023-01-01",
            "end_date": "2023-12-31",
            "report_types": ["10-K", "10-Q"]
        }
        "#;

        let query = Query::from_json(json_str).unwrap();

        assert_eq!(query.tickers, vec!["AAPL", "GOOGL"]);
        assert_eq!(
            query.start_date,
            NaiveDate::from_ymd_opt(2023, 1, 1).unwrap()
        );
        assert_eq!(
            query.end_date,
            NaiveDate::from_ymd_opt(2023, 12, 31).unwrap()
        );
        assert_eq!(
            query.report_types,
            vec![report::ReportType::Form10K, report::ReportType::Form10Q]
        );
    }
}

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Query parameters for fetching earnings call transcripts
/// 
/// # JSON Format
/// The deserializer accepts JSON in this format:
/// ```json
/// {
///   "ticker": "AAPL",
///   "start_date": "2024-01-01",
///   "end_date": "2024-03-31"
/// }
/// ```
/// 
/// Example for fetching Tesla's Q4 2023 earnings call:
/// ```json
/// {
///   "ticker": "TSLA",
///   "start_date": "2023-12-01",  // Start of Q4
///   "end_date": "2024-01-31"     // Allow time for transcript publication
/// }
/// ```
/// 
/// Fields:
/// - `ticker`: Company stock ticker symbol (string)
/// - `start_date`: Start date in YYYY-MM-DD format
/// - `end_date`: End date in YYYY-MM-DD format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub ticker: String,
    #[serde(with = "crate::query::date_format")]
    pub start_date: NaiveDate,
    #[serde(with = "crate::query::date_format")]
    pub end_date: NaiveDate,
}

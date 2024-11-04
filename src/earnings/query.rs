use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Query parameters for fetching earnings call transcripts
/// 
/// # JSON Format
/// ```json
/// {
///   "ticker": "AAPL",
///   "start_date": "2024-01-01",
///   "end_date": "2024-03-31"
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

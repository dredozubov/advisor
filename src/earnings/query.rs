use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub ticker: String,
    #[serde(with = "crate::query::date_format")]
    pub start_date: NaiveDate,
    #[serde(with = "crate::query::date_format")]
    pub end_date: NaiveDate,
}

use crate::edgar;
use crate::earnings;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub tickers: Vec<String>,
    #[serde(with = "date_format")]
    pub start_date: NaiveDate,
    #[serde(with = "date_format")]
    pub end_date: NaiveDate,
    pub edgar_query: Option<edgar::query::Query>,
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

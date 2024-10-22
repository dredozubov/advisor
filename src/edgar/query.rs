use chrono::NaiveDate;

pub struct Query {
    pub tickers: Vec<String>,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub report_types: Vec<String>,
}

impl Query {
    pub fn new(
        tickers: Vec<String>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        report_types: Vec<String>,
    ) -> Self {
        Query {
            tickers,
            start_date,
            end_date,
            report_types,
        }
    }
}

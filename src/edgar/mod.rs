mod tickers;

use anyhow::{anyhow, Result};
use sec_edgar::{
    edgar::{edgar_client, get_feed_entries},
    edgar_query::{
        edgar_query_builder::{BuilderInput, EdgarQueryBuilder},
        filing::FilingTypeOption::_10Q,
    },
};
use tickers::TICKER_TO_CIK;

pub async fn get_latest_10q(ticker: &str) -> Result<String> {
    let cik = TICKER_TO_CIK.get(ticker).ok_or_else(|| anyhow!("Ticker not found: {}", ticker))?;
    println!("CIK: {:?}", cik);
    let query = EdgarQueryBuilder::new(cik)
        .set_filing_type(BuilderInput::TypeTInput(_10Q))
        .build()?;
    println!("query: {:?}", query);
    let entries = get_feed_entries(edgar_client()?, query).await?;
    println!("entries: {:?}", entries);

    // For now, we'll just return OK. In a real implementation, you'd process the entries here.
    Ok(String::from("OK"))
}

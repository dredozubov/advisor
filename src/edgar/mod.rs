use anyhow::{anyhow, Result};
use tickers::TICKER_TO_CIK;
use sec_edgar::{
    edgar::{edgar_client, get_feed_entries, get_feed_entry_content},
    edgar_query::{
        cik_query::CIKQuery,
        edgar_query_builder::{BuilderInput, EdgarQueryBuilder},
        filing::FilingTypeOption::_10Q,
    },
};

pub async fn get_latest_10q(ticker: &str) -> Result<String> {
    TICKER_TO_CIK
    println!("CIK: {:?}", cik);
    let query = EdgarQueryBuilder::new(&cik)
        .set_filing_type(BuilderInput::TypeTInput(_10Q))
        .build()
        .unwrap();
    println!("query: {:?}", query);
    let entries = get_feed_entries(edgar_client().unwrap(), query)
        .await
        .unwrap_err();
    println!("entries: {:?}", entries);
    // let filing_type = get_feed_entry_content(entries.first().unwrap())
    //     .unwrap()
    //     .filing_type
    //     .value;

    // Ok(filing_type)
    Ok(String::from("OK"))
}

use anyhow::{anyhow, Result};
use sec_edgar::{
    filings::{Filing, FilingType},
    client::EdgarClient,
};

mod tickers;
use tickers::TICKER_TO_CIK;

pub async fn get_latest_10q(ticker: &str) -> Result<String> {
    let cik = TICKER_TO_CIK.get(ticker)
        .ok_or_else(|| anyhow!("Ticker not found: {}", ticker))?;

    let client = EdgarClient::new();
    let filings = client.get_filings(&cik.to_string()).await?;

    let latest_10q = filings.into_iter()
        .filter(|filing| filing.filing_type == FilingType::Filing10Q)
        .max_by_key(|filing| filing.date_filed)
        .ok_or_else(|| anyhow!("No 10-Q filing found for ticker: {}", ticker))?;

    let content = client.get_filing_contents(&latest_10q).await?;

    Ok(content)
}

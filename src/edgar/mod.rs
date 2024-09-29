mod tickers;

use edgar::Edgar;
use crate::edgar::tickers::TICKER_TO_ID;
use anyhow::{Result, anyhow};

pub async fn get_latest_10q(ticker: &str) -> Result<String> {
    let edgar = Edgar::new();
    
    let cik = TICKER_TO_ID.get(ticker)
        .ok_or_else(|| anyhow!("Ticker not found: {}", ticker))?;

    let filings = edgar.company_filings(*cik).await?;
    
    let latest_10q = filings.iter()
        .find(|filing| filing.form == "10-Q")
        .ok_or_else(|| anyhow!("No 10-Q filing found for ticker: {}", ticker))?;

    let content = edgar.filing_content(&latest_10q.url).await?;
    
    Ok(content)
}

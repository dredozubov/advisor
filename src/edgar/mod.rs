use anyhow::{anyhow, Result};
use sec_edgar::{
    filings::{Filing, FilingType},
    client::EdgarClient,
    search::CIKQuery,
};

pub async fn get_latest_10q(ticker: &str) -> Result<String> {
    let cik_query = CIKQuery::new(Some("./src/edgar/tickers"));
    let cik = cik_query.get_cik(ticker)
        .ok_or_else(|| anyhow!("Ticker not found: {}", ticker))?;

    let client = EdgarClient::new();
    let filings = client.get_filings(&cik).await?;

    let latest_10q = filings.into_iter()
        .filter(|filing| filing.filing_type == FilingType::Filing10Q)
        .max_by_key(|filing| filing.date_filed)
        .ok_or_else(|| anyhow!("No 10-Q filing found for ticker: {}", ticker))?;

    let content = client.get_filing_contents(&latest_10q).await?;

    Ok(content)
}

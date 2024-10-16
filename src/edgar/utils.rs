use anyhow::Result;
use reqwest::Client;
use std::path::Path;
use url::Url;

pub async fn fetch_and_save(
    client: &Client,
    url: &Url,
    filepath: &Path,
    user_agent: &str,
) -> Result<()> {
    let response = client
        .get(url.as_str())
        .header(reqwest::header::USER_AGENT, user_agent)
        .send()
        .await?;
    let content = response.bytes().await?;
    std::fs::write(filepath, content)?;
    Ok(())
}

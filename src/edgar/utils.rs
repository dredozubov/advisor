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
    log::debug!("Fetching URL: {}", url);
    
    let response = client
        .get(url.as_str())
        .header(reqwest::header::USER_AGENT, user_agent)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::ACCEPT_ENCODING, "gzip, deflate")
        .send()
        .await?;

    log::debug!("Response status: {}", response.status());
    log::debug!("Response headers: {:?}", response.headers());

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "HTTP request failed with status: {}",
            response.status()
        ));
    }

    let content = response.bytes().await?;
    log::debug!("Received content length: {}", content.len());

    std::fs::write(filepath, &content)?;
    log::debug!("Saved content to {:?}", filepath);

    // Verify the saved content
    let saved_content = std::fs::read(filepath)?;
    log::debug!("Verified saved content length: {}", saved_content.len());
    
    if saved_content.len() != content.len() {
        return Err(anyhow::anyhow!(
            "Content length mismatch: received {} bytes but saved {} bytes",
            content.len(),
            saved_content.len()
        ));
    }

    Ok(())
}

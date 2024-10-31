use anyhow::Result;
use mime::Mime;
use reqwest::{header::HeaderValue, Client};
use std::path::Path;
use url::Url;
use super::rate_limiter::RateLimiter;

pub async fn fetch_and_save(
    client: &Client,
    url: &Url,
    filepath: &Path,
    user_agent: &str,
    content_type: Mime,
) -> Result<()> {
    log::debug!("Fetching URL: {}", url);
    
    // Acquire rate limit permit before making request
    let rate_limiter = RateLimiter::global();
    let _permit = rate_limiter.acquire().await;

    let content_type_value = HeaderValue::from_str(content_type.as_ref())?;
    let request = client
        .get(url.as_str())
        .header(reqwest::header::USER_AGENT, user_agent)
        .header(reqwest::header::ACCEPT_ENCODING, "gzip, deflate")
        .header(reqwest::header::ACCEPT, &content_type_value)
        .header(reqwest::header::CONTENT_TYPE, &content_type_value);

    let response = request.send().await?;

    log::debug!("Response status: {}", response.status());
    log::debug!("Response headers: {:?}", response.headers());

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "HTTP request failed with status: {}",
            response.status()
        ));
    }

    // Get the content length from headers if available
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|cl| cl.to_str().ok())
        .and_then(|cl| cl.parse::<usize>().ok());

    if let Some(length) = content_length {
        log::debug!("Expected content length: {}", length);
    }

    // Read the full response body as text
    let content = response.text().await?;
    log::debug!("Received content length: {}", content.len());

    std::fs::write(filepath, &content)?;
    log::debug!("Saved content to {:?}", filepath);

    // Verify the saved content
    let saved_content = std::fs::read_to_string(filepath)?;
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

use anyhow::{Context, Result};
use serde_json::json;

pub async fn post_message(
    client: &reqwest::Client,
    webhook_url_env: &str,
    text: &str,
) -> Result<()> {
    let webhook_url = std::env::var(webhook_url_env)
        .with_context(|| format!("environment variable {webhook_url_env} not set"))?;

    post_to_url(client, &webhook_url, text).await
}

pub async fn post_to_url(
    client: &reqwest::Client,
    webhook_url: &str,
    text: &str,
) -> Result<()> {
    let response = client
        .post(webhook_url)
        .json(&json!({ "text": text }))
        .send()
        .await
        .context("failed to post to Slack webhook")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Slack webhook returned {status}: {body}");
    }

    tracing::info!("Delivered message to Slack");
    Ok(())
}

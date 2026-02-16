pub mod slack;

use anyhow::Result;

use crate::config::SinkConfig;

pub async fn deliver(
    sink: &SinkConfig,
    text: &str,
    http_client: &reqwest::Client,
) -> Result<()> {
    match sink {
        SinkConfig::Slack { webhook_url_env } => {
            slack::post_message(http_client, webhook_url_env, text).await
        }
    }
}

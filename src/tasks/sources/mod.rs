pub mod rss;

use chrono::{DateTime, Utc};

use crate::config::SourceConfig;

#[derive(Debug, Clone)]
pub struct ContentItem {
    pub title: String,
    pub url: String,
    pub summary: String,
    pub published: Option<DateTime<Utc>>,
}

pub async fn fetch_all(
    sources: &[SourceConfig],
    http_client: &reqwest::Client,
) -> Vec<ContentItem> {
    let mut items = Vec::new();
    for source in sources {
        match source {
            SourceConfig::Rss { url, limit } => match rss::fetch_feed(http_client, url, *limit).await {
                Ok(feed_items) => {
                    tracing::info!(url = %url, count = feed_items.len(), "Fetched RSS feed");
                    items.extend(feed_items);
                }
                Err(e) => {
                    tracing::error!(url = %url, error = %e, "Failed to fetch RSS feed");
                }
            },
        }
    }
    items
}

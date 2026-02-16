pub mod rss;

use chrono::{DateTime, Utc};
use futures::future::join_all;

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
    let futures: Vec<_> = sources
        .iter()
        .map(|source| async move {
            match source {
                SourceConfig::Rss { url, limit } => {
                    match rss::fetch_feed(http_client, url, *limit).await {
                        Ok(feed_items) => {
                            tracing::info!(url = %url, count = feed_items.len(), "Fetched RSS feed");
                            feed_items
                        }
                        Err(e) => {
                            tracing::error!(url = %url, error = %e, "Failed to fetch RSS feed");
                            Vec::new()
                        }
                    }
                }
            }
        })
        .collect();

    join_all(futures).await.into_iter().flatten().collect()
}

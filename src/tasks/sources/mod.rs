pub mod coingecko;
pub mod github_prs;
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
    pub image_url: Option<String>,
}

pub async fn fetch_all(
    sources: &[SourceConfig],
    http_client: &reqwest::Client,
    github_token: Option<&str>,
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
                SourceConfig::GithubMergedPrs { repos, since_days } => {
                    let Some(token) = github_token else {
                        tracing::error!("GithubMergedPrs source requires GITHUB_TOKEN but none is set");
                        return Vec::new();
                    };
                    match github_prs::fetch_merged_prs(http_client, token, repos, *since_days).await {
                        Ok(items) => {
                            tracing::info!(repos = ?repos, count = items.len(), "Fetched merged PRs");
                            items
                        }
                        Err(e) => {
                            tracing::error!(repos = ?repos, error = %e, "Failed to fetch merged PRs");
                            Vec::new()
                        }
                    }
                }
            }
        })
        .collect();

    join_all(futures).await.into_iter().flatten().collect()
}

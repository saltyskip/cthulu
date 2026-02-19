pub mod market;
pub mod github_prs;
pub mod rss;
pub mod web_scrape;

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

fn keyword_matches(item: &ContentItem, keywords: &[String]) -> bool {
    if keywords.is_empty() {
        return true;
    }
    let haystack = format!("{} {}", item.title, item.summary).to_lowercase();
    keywords.iter().any(|kw| haystack.contains(&kw.to_lowercase()))
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
                SourceConfig::Rss { url, limit, keywords } => {
                    match rss::fetch_feed(http_client, url, *limit).await {
                        Ok(feed_items) => {
                            let filtered: Vec<_> = feed_items
                                .into_iter()
                                .filter(|item| keyword_matches(item, keywords))
                                .collect();
                            tracing::debug!(url = %url, count = filtered.len(), "Fetched RSS feed");
                            filtered
                        }
                        Err(e) => {
                            tracing::warn!(url = %url, error = %e, "Failed to fetch RSS feed");
                            Vec::new()
                        }
                    }
                }
                SourceConfig::WebScrape { url, keywords } => {
                    match web_scrape::fetch_page_text(http_client, url).await {
                        Ok(items) => {
                            let filtered: Vec<_> = items
                                .into_iter()
                                .filter(|item| keyword_matches(item, keywords))
                                .collect();
                            tracing::debug!(url = %url, count = filtered.len(), "Fetched web page");
                            filtered
                        }
                        Err(e) => {
                            tracing::warn!(url = %url, error = %e, "Failed to fetch web page");
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
                            tracing::debug!(repos = ?repos, count = items.len(), "Fetched merged PRs");
                            items
                        }
                        Err(e) => {
                            tracing::error!(repos = ?repos, error = %e, "Failed to fetch merged PRs");
                            Vec::new()
                        }
                    }
                }
                SourceConfig::WebScraper {
                    url, base_url, items_selector, title_selector,
                    url_selector, summary_selector, date_selector,
                    date_format, limit,
                } => {
                    match web_scrape::fetch_page(
                        http_client, url, items_selector,
                        title_selector.as_deref(), url_selector.as_deref(),
                        summary_selector.as_deref(), date_selector.as_deref(),
                        date_format.as_deref(), *limit, base_url.as_deref(),
                    ).await {
                        Ok(items) => {
                            tracing::debug!(url = %url, count = items.len(), "Fetched web scrape");
                            items
                        }
                        Err(e) => {
                            tracing::error!(url = %url, error = %e, "Failed to scrape page");
                            Vec::new()
                        }
                    }
                }
            }
        })
        .collect();

    join_all(futures).await.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(title: &str, summary: &str) -> ContentItem {
        ContentItem {
            title: title.to_string(),
            url: String::new(),
            summary: summary.to_string(),
            published: None,
            image_url: None,
        }
    }

    #[test]
    fn test_keyword_matches_empty_keywords() {
        let item = make_item("anything", "goes");
        assert!(keyword_matches(&item, &[]));
    }

    #[test]
    fn test_keyword_matches_title() {
        let item = make_item("Bitcoin surges to new high", "market update");
        assert!(keyword_matches(&item, &["bitcoin".to_string()]));
    }

    #[test]
    fn test_keyword_matches_summary() {
        let item = make_item("Market Update", "ethereum hits resistance level");
        assert!(keyword_matches(&item, &["ethereum".to_string()]));
    }

    #[test]
    fn test_keyword_matches_case_insensitive() {
        let item = make_item("BITCOIN news", "");
        assert!(keyword_matches(&item, &["bitcoin".to_string()]));
        assert!(keyword_matches(&item, &["Bitcoin".to_string()]));
        assert!(keyword_matches(&item, &["BITCOIN".to_string()]));
    }

    #[test]
    fn test_keyword_no_match() {
        let item = make_item("Stock market update", "S&P 500 rallies");
        assert!(!keyword_matches(&item, &["bitcoin".to_string(), "crypto".to_string()]));
    }

    #[test]
    fn test_keyword_any_match() {
        let item = make_item("Crypto regulation", "new laws proposed");
        assert!(keyword_matches(&item, &["bitcoin".to_string(), "crypto".to_string()]));
    }
}

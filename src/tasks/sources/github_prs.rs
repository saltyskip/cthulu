use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::ContentItem;

#[derive(Deserialize)]
struct SearchResponse {
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    title: String,
    html_url: String,
    body: Option<String>,
    pull_request: Option<PullRequestRef>,
    #[serde(rename = "created_at")]
    _created_at: Option<String>,
}

#[derive(Deserialize)]
struct PullRequestRef {
    merged_at: Option<DateTime<Utc>>,
}

pub async fn fetch_merged_prs(
    http_client: &reqwest::Client,
    token: &str,
    repos: &[String],
    since_days: u64,
) -> Result<Vec<ContentItem>> {
    let since_date = Utc::now() - chrono::Duration::days(since_days as i64);
    let date_str = since_date.format("%Y-%m-%d").to_string();

    // Build query: repo:owner/a repo:owner/b is:pr is:merged merged:>=YYYY-MM-DD
    let repo_clauses: Vec<String> = repos.iter().map(|r| format!("repo:{r}")).collect();
    let query = format!(
        "{} is:pr is:merged merged:>={}",
        repo_clauses.join(" "),
        date_str
    );

    let mut all_items = Vec::new();
    let mut page = 1u32;
    let per_page = 100;
    let max_items = 500;

    loop {
        let resp = http_client
            .get("https://api.github.com/search/issues")
            .query(&[
                ("q", query.as_str()),
                ("per_page", &per_page.to_string()),
                ("page", &page.to_string()),
                ("sort", "updated"),
                ("order", "desc"),
            ])
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "cthulu-bot")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .context("GitHub search API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub search API returned {status}: {body}");
        }

        let search: SearchResponse = resp.json().await.context("Failed to parse search response")?;

        if search.items.is_empty() {
            break;
        }

        for item in search.items {
            let merged_at = item
                .pull_request
                .as_ref()
                .and_then(|pr| pr.merged_at);

            all_items.push(ContentItem {
                title: item.title,
                url: item.html_url,
                summary: item.body.unwrap_or_default(),
                published: merged_at,
            });
        }

        if all_items.len() >= max_items {
            all_items.truncate(max_items);
            break;
        }

        page += 1;
        if page > 5 {
            break;
        }
    }

    Ok(all_items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_search_response() {
        let json = r#"{
            "total_count": 2,
            "incomplete_results": false,
            "items": [
                {
                    "title": "Fix login bug",
                    "html_url": "https://github.com/owner/repo/pull/42",
                    "body": "Fixed the login timeout issue",
                    "created_at": "2025-01-15T10:00:00Z",
                    "pull_request": {
                        "merged_at": "2025-01-16T12:00:00Z"
                    }
                },
                {
                    "title": "Add dark mode",
                    "html_url": "https://github.com/owner/repo/pull/43",
                    "body": null,
                    "created_at": "2025-01-15T11:00:00Z",
                    "pull_request": {
                        "merged_at": null
                    }
                }
            ]
        }"#;

        let resp: SearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.items.len(), 2);
        assert_eq!(resp.items[0].title, "Fix login bug");
        assert_eq!(
            resp.items[0].html_url,
            "https://github.com/owner/repo/pull/42"
        );
        assert_eq!(
            resp.items[0].body.as_deref(),
            Some("Fixed the login timeout issue")
        );
        assert!(resp.items[0].pull_request.as_ref().unwrap().merged_at.is_some());
        assert!(resp.items[1].body.is_none());
        assert!(resp.items[1].pull_request.as_ref().unwrap().merged_at.is_none());
    }
}

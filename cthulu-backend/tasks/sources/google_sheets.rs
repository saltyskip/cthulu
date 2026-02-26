use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

use super::ContentItem;

#[derive(Deserialize)]
struct SheetResponse {
    #[serde(default)]
    values: Vec<Vec<String>>,
}

pub async fn fetch_sheet(
    client: &reqwest::Client,
    spreadsheet_id: &str,
    range: Option<&str>,
    service_account_key_path: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<ContentItem>> {
    let range = range.unwrap_or("Sheet1");
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{spreadsheet_id}/values/{range}"
    );

    let mut request = client.get(&url);

    if let Some(key_path) = service_account_key_path {
        use gcp_auth::TokenProvider;
        let sa = gcp_auth::CustomServiceAccount::from_file(key_path)
            .context("Failed to load service account key file")?;
        let token = sa
            .token(&["https://www.googleapis.com/auth/spreadsheets.readonly"])
            .await
            .context("Failed to get Google access token")?;
        request = request.bearer_auth(token.as_str());
    }

    let resp = request.send().await.context("Failed to fetch Google Sheets")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Google Sheets API returned {status}: {body}");
    }

    let sheet: SheetResponse = resp.json().await.context("Failed to parse Sheets response")?;
    parse_rows(&sheet.values, limit)
}

fn find_column(headers: &[String], candidates: &[&str]) -> Option<usize> {
    headers.iter().position(|h| {
        let lower = h.to_lowercase();
        candidates.iter().any(|c| lower.contains(c))
    })
}

fn parse_rows(values: &[Vec<String>], limit: Option<usize>) -> Result<Vec<ContentItem>> {
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let headers = &values[0];
    let title_col = find_column(headers, &["title", "post", "content", "name"]);
    let url_col = find_column(headers, &["url", "link"]);
    let date_col = find_column(headers, &["date", "published", "posted"]);

    let data_rows = &values[1..];
    let rows_to_process = match limit {
        Some(n) => &data_rows[..data_rows.len().min(n)],
        None => data_rows,
    };

    let items = rows_to_process
        .iter()
        .filter(|row| !row.is_empty())
        .map(|row| {
            let get = |idx: Option<usize>| -> String {
                idx.and_then(|i| row.get(i))
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            };

            let title = if let Some(col) = title_col {
                get(Some(col))
            } else {
                row.first().cloned().unwrap_or_default()
            };

            let url_val = get(url_col);

            // Build summary from all columns as key-value pairs
            let summary = headers
                .iter()
                .enumerate()
                .filter_map(|(i, header)| {
                    row.get(i)
                        .filter(|v| !v.is_empty())
                        .map(|v| format!("{header}: {v}"))
                })
                .collect::<Vec<_>>()
                .join("\n");

            let published = date_col
                .and_then(|i| row.get(i))
                .and_then(|d| parse_date(d));

            ContentItem {
                title,
                url: url_val,
                summary,
                published,
                image_url: None,
            }
        })
        .collect();

    Ok(items)
}

fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    // Try ISO 8601 first, then common date formats
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Some(dt);
    }
    for fmt in &["%Y-%m-%d", "%m/%d/%Y", "%d/%m/%Y", "%b %d, %Y"] {
        if let Ok(nd) = NaiveDate::parse_from_str(s, fmt) {
            return nd.and_hms_opt(0, 0, 0).map(|ndt| ndt.and_utc());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rows_basic() {
        let values = vec![
            vec!["Title".into(), "URL".into(), "Views".into(), "Date".into()],
            vec!["Post A".into(), "https://example.com/a".into(), "1000".into(), "2025-01-15".into()],
            vec!["Post B".into(), "https://example.com/b".into(), "500".into(), "2025-01-16".into()],
        ];

        let items = parse_rows(&values, None).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Post A");
        assert_eq!(items[0].url, "https://example.com/a");
        assert!(items[0].summary.contains("Views: 1000"));
        assert!(items[0].published.is_some());
    }

    #[test]
    fn test_parse_rows_with_limit() {
        let values = vec![
            vec!["Name".into()],
            vec!["Row 1".into()],
            vec!["Row 2".into()],
            vec!["Row 3".into()],
        ];

        let items = parse_rows(&values, Some(2)).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_parse_rows_empty() {
        let items = parse_rows(&[], None).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn test_find_column_case_insensitive() {
        let headers = vec!["Platform".into(), "Post Title".into(), "Link".into()];
        assert_eq!(find_column(&headers, &["title", "post", "name"]), Some(1));
        assert_eq!(find_column(&headers, &["url", "link"]), Some(2));
    }

    #[test]
    fn test_parse_date_formats() {
        assert!(parse_date("2025-01-15").is_some());
        assert!(parse_date("01/15/2025").is_some());
        assert!(parse_date("Jan 15, 2025").is_some());
        assert!(parse_date("not a date").is_none());
    }
}

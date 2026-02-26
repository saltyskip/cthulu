use anyhow::{Context, Result};
use scraper::{Html, Selector};

use super::ContentItem;

pub async fn fetch_page(
    client: &reqwest::Client,
    url: &str,
    items_selector: &str,
    title_selector: Option<&str>,
    url_selector: Option<&str>,
    summary_selector: Option<&str>,
    date_selector: Option<&str>,
    date_format: Option<&str>,
    limit: usize,
    base_url: Option<&str>,
) -> Result<Vec<ContentItem>> {
    let html = client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("failed to fetch page")?
        .error_for_status()
        .with_context(|| format!("page returned error status: {url}"))?
        .text()
        .await
        .context("failed to read page body")?;

    parse_page(&html, items_selector, title_selector, url_selector, summary_selector, date_selector, date_format, limit, base_url)
}

fn parse_page(
    html: &str,
    items_selector: &str,
    title_selector: Option<&str>,
    url_selector: Option<&str>,
    summary_selector: Option<&str>,
    date_selector: Option<&str>,
    date_format: Option<&str>,
    limit: usize,
    base_url: Option<&str>,
) -> Result<Vec<ContentItem>> {
    let document = Html::parse_document(html);
    let items_sel = Selector::parse(items_selector)
        .map_err(|e| anyhow::anyhow!("invalid items selector '{}': {:?}", items_selector, e))?;

    let title_sel = title_selector
        .map(|s| Selector::parse(s).map_err(|e| anyhow::anyhow!("invalid title selector: {:?}", e)))
        .transpose()?;
    let url_sel = url_selector
        .map(|s| Selector::parse(s).map_err(|e| anyhow::anyhow!("invalid url selector: {:?}", e)))
        .transpose()?;
    let summary_sel = summary_selector
        .map(|s| Selector::parse(s).map_err(|e| anyhow::anyhow!("invalid summary selector: {:?}", e)))
        .transpose()?;
    let date_sel = date_selector
        .map(|s| Selector::parse(s).map_err(|e| anyhow::anyhow!("invalid date selector: {:?}", e)))
        .transpose()?;

    let mut results = Vec::new();

    for element in document.select(&items_sel).take(limit) {
        let title = title_sel
            .as_ref()
            .and_then(|sel| element.select(sel).next())
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let raw_url = url_sel
            .as_ref()
            .and_then(|sel| element.select(sel).next())
            .and_then(|el| el.value().attr("href").map(String::from))
            .unwrap_or_default();

        let item_url = resolve_url(&raw_url, base_url);

        let summary = summary_sel
            .as_ref()
            .and_then(|sel| element.select(sel).next())
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let published = date_sel
            .as_ref()
            .and_then(|sel| element.select(sel).next())
            .and_then(|el| {
                let text = el.text().collect::<String>();
                let text = text.trim();
                if let Some(fmt) = date_format {
                    chrono::NaiveDateTime::parse_from_str(text, fmt)
                        .ok()
                        .map(|dt| dt.and_utc())
                        .or_else(|| {
                            chrono::NaiveDate::parse_from_str(text, fmt)
                                .ok()
                                .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
                        })
                } else {
                    chrono::DateTime::parse_from_rfc3339(text)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .or_else(|| {
                            chrono::DateTime::parse_from_rfc2822(text)
                                .ok()
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                        })
                }
            });

        if title.is_empty() && item_url.is_empty() {
            continue;
        }

        results.push(ContentItem {
            title,
            url: item_url,
            summary,
            published,
            image_url: None,
        });
    }

    Ok(results)
}

fn resolve_url(raw: &str, base_url: Option<&str>) -> String {
    if raw.is_empty() {
        return String::new();
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return raw.to_string();
    }
    if let Some(base) = base_url {
        let base = base.trim_end_matches('/');
        if raw.starts_with('/') {
            format!("{base}{raw}")
        } else {
            format!("{base}/{raw}")
        }
    } else {
        raw.to_string()
    }
}

/// Simple full-page text fetcher for `WebScrape` source variant.
/// Strips all HTML tags and returns the page body as a single ContentItem.
pub async fn fetch_page_text(client: &reqwest::Client, url: &str) -> Result<Vec<ContentItem>> {
    let html = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (compatible; Cthulu/1.0)")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("failed to fetch page")?
        .error_for_status()
        .with_context(|| format!("page returned error status: {url}"))?
        .text()
        .await
        .context("failed to read page body")?;

    let title = extract_title(&html).unwrap_or_else(|| url.to_string());
    let body = strip_html(&html);

    Ok(vec![ContentItem {
        title,
        url: url.to_string(),
        summary: body,
        published: None,
        image_url: None,
    }])
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let tag_close = lower[start..].find('>')?;
    let content_start = start + tag_close + 1;
    let end = lower[content_start..].find("</title>")?;
    let title = html[content_start..content_start + end].trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

fn strip_html(html: &str) -> String {
    let document = Html::parse_document(html);
    let body_sel = Selector::parse("body").unwrap();
    let script_sel = Selector::parse("script, style, noscript").unwrap();

    let text = document
        .select(&body_sel)
        .next()
        .map(|body| {
            let script_ids: std::collections::HashSet<_> = body
                .select(&script_sel)
                .map(|el| el.id())
                .collect();
            body.descendants()
                .filter_map(|node| {
                    if let scraper::node::Node::Text(t) = node.value() {
                        // Skip text inside script/style
                        let dominated = node.ancestors().any(|a| script_ids.contains(&a.id()));
                        if dominated { None } else { Some(t.text.as_ref()) }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();

    // Collapse whitespace
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");

    // Cap at 50k chars
    if collapsed.len() > 50_000 {
        collapsed[..50_000].to_string()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_HTML: &str = r#"
    <html>
    <body>
        <div class="press-release">
            <h3><a href="/news/release/2024-01">SEC Approves Spot Bitcoin ETF</a></h3>
            <p class="summary">The SEC has approved multiple spot Bitcoin ETF applications.</p>
            <span class="date">2024-01-10</span>
        </div>
        <div class="press-release">
            <h3><a href="https://example.com/news/2">CFTC Issues Guidance on DeFi</a></h3>
            <p class="summary">New guidance clarifies CFTC jurisdiction over DeFi protocols.</p>
            <span class="date">2024-01-11</span>
        </div>
        <div class="press-release">
            <h3><a href="/news/release/2024-03">Treasury Report on Stablecoins</a></h3>
            <p class="summary">Treasury releases comprehensive stablecoin risk assessment.</p>
            <span class="date">2024-01-12</span>
        </div>
    </body>
    </html>
    "#;

    #[test]
    fn test_parse_page_basic() {
        let items = parse_page(
            FIXTURE_HTML,
            "div.press-release",
            Some("h3"),
            Some("h3 a"),
            Some("p.summary"),
            None,
            None,
            10,
            Some("https://www.sec.gov"),
        )
        .unwrap();

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].title, "SEC Approves Spot Bitcoin ETF");
        assert_eq!(items[0].url, "https://www.sec.gov/news/release/2024-01");
        assert_eq!(
            items[0].summary,
            "The SEC has approved multiple spot Bitcoin ETF applications."
        );
    }

    #[test]
    fn test_parse_page_absolute_url_preserved() {
        let items = parse_page(
            FIXTURE_HTML,
            "div.press-release",
            Some("h3"),
            Some("h3 a"),
            None,
            None,
            None,
            10,
            Some("https://www.sec.gov"),
        )
        .unwrap();

        // Second item has absolute URL, should not be prefixed
        assert_eq!(items[1].url, "https://example.com/news/2");
    }

    #[test]
    fn test_parse_page_limit() {
        let items = parse_page(
            FIXTURE_HTML,
            "div.press-release",
            Some("h3"),
            Some("h3 a"),
            None,
            None,
            None,
            2,
            None,
        )
        .unwrap();

        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_parse_page_with_date() {
        let items = parse_page(
            FIXTURE_HTML,
            "div.press-release",
            Some("h3"),
            Some("h3 a"),
            None,
            Some("span.date"),
            Some("%Y-%m-%d"),
            10,
            None,
        )
        .unwrap();

        assert!(items[0].published.is_some());
        assert_eq!(
            items[0].published.unwrap().format("%Y-%m-%d").to_string(),
            "2024-01-10"
        );
    }

    #[test]
    fn test_resolve_url_absolute() {
        assert_eq!(
            resolve_url("https://example.com/page", Some("https://base.com")),
            "https://example.com/page"
        );
    }

    #[test]
    fn test_resolve_url_relative() {
        assert_eq!(
            resolve_url("/path/page", Some("https://base.com")),
            "https://base.com/path/page"
        );
    }

    #[test]
    fn test_resolve_url_no_base() {
        assert_eq!(resolve_url("/path/page", None), "/path/page");
    }

    #[test]
    fn test_resolve_url_empty() {
        assert_eq!(resolve_url("", Some("https://base.com")), "");
    }

    #[test]
    fn test_invalid_selector() {
        let result = parse_page(
            FIXTURE_HTML,
            "[[[invalid",
            None,
            None,
            None,
            None,
            None,
            10,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_strip_html_basic() {
        let html = "<html><body><p>Hello <b>world</b></p></body></html>";
        let result = strip_html(html);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_strip_html_removes_tags() {
        let html = "<html><body><p>Before</p><div class='big'>Middle</div><p>After</p></body></html>";
        let result = strip_html(html);
        assert!(result.contains("Before"));
        assert!(result.contains("Middle"));
        assert!(result.contains("After"));
        assert!(!result.contains("<p>"));
        assert!(!result.contains("<div"));
    }

    #[test]
    fn test_strip_html_caps_at_50k() {
        let long_text = "a ".repeat(30_000);
        let html = format!("<html><body>{long_text}</body></html>");
        let result = strip_html(&html);
        assert!(result.len() <= 50_000);
    }

    #[test]
    fn test_extract_title_basic() {
        let html = "<html><head><title>My Page</title></head><body></body></html>";
        assert_eq!(extract_title(html), Some("My Page".to_string()));
    }

    #[test]
    fn test_extract_title_missing() {
        let html = "<html><head></head><body></body></html>";
        assert_eq!(extract_title(html), None);
    }

    #[test]
    fn test_extract_title_empty() {
        let html = "<html><head><title></title></head><body></body></html>";
        assert_eq!(extract_title(html), None);
    }
}

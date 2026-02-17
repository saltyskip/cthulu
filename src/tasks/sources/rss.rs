use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures::future::join_all;

use super::ContentItem;

pub async fn fetch_feed(
    client: &reqwest::Client,
    url: &str,
    limit: usize,
) -> Result<Vec<ContentItem>> {
    let bytes = client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("failed to fetch feed")?
        .error_for_status()
        .with_context(|| format!("feed returned error status: {url}"))?
        .bytes()
        .await
        .context("failed to read feed body")?;

    let feed = feed_rs::parser::parse(&bytes[..]).context("failed to parse feed")?;

    let mut items: Vec<ContentItem> = feed
        .entries
        .into_iter()
        .take(limit)
        .map(|entry| {
            let title = entry
                .title
                .map(|t| t.content)
                .unwrap_or_default();
            let url = entry
                .links
                .first()
                .map(|l| l.href.clone())
                .unwrap_or_default();
            let summary = entry
                .summary
                .map(|s| s.content)
                .or_else(|| entry.content.and_then(|c| c.body))
                .unwrap_or_default();
            let published: Option<DateTime<Utc>> = entry
                .published
                .or(entry.updated);

            ContentItem {
                title,
                url,
                summary,
                published,
                image_url: None,
            }
        })
        .collect();

    // Concurrently fetch og:image for each item (best-effort, 15s overall timeout)
    let futures: Vec<_> = items
        .iter()
        .map(|item| extract_og_image(client, &item.url))
        .collect();

    let results = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        join_all(futures),
    )
    .await
    .unwrap_or_else(|_| vec![None; items.len()]);

    for (item, image_url) in items.iter_mut().zip(results) {
        item.image_url = image_url;
    }

    Ok(items)
}

async fn extract_og_image(client: &reqwest::Client, url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    let html = client
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    extract_og_image_from_html(&html)
}

fn extract_og_image_from_html(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let mut search_from = 0;
    while let Some(meta_pos) = lower[search_from..].find("<meta") {
        let abs_pos = search_from + meta_pos;
        let tag_end = match lower[abs_pos..].find('>') {
            Some(e) => abs_pos + e,
            None => break,
        };
        let tag = &html[abs_pos..=tag_end];
        let tag_lower = &lower[abs_pos..=tag_end];

        if tag_lower.contains("og:image") && !tag_lower.contains("og:image:") {
            if let Some(content_start) = tag_lower.find("content=") {
                let rest = &tag[content_start + 8..];
                let (quote, rest) = if rest.starts_with('"') {
                    ('"', &rest[1..])
                } else if rest.starts_with('\'') {
                    ('\'', &rest[1..])
                } else {
                    search_from = tag_end;
                    continue;
                };
                if let Some(end) = rest.find(quote) {
                    let url = rest[..end].trim().to_string();
                    if !url.is_empty() {
                        return Some(url);
                    }
                }
            }
        }
        search_from = tag_end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rss2_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test Feed</title>
            <item>
              <title>Article One</title>
              <link>https://example.com/1</link>
              <description>Summary of article one</description>
              <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
            </item>
            <item>
              <title>Article Two</title>
              <link>https://example.com/2</link>
              <description>Summary of article two</description>
            </item>
          </channel>
        </rss>"#;

        let feed = feed_rs::parser::parse(xml.as_bytes()).unwrap();
        assert_eq!(feed.entries.len(), 2);

        let entry = &feed.entries[0];
        assert_eq!(entry.title.as_ref().unwrap().content, "Article One");
        assert_eq!(entry.links[0].href, "https://example.com/1");
    }

    #[test]
    fn test_parse_atom_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>Test Atom Feed</title>
          <entry>
            <title>Atom Entry</title>
            <link href="https://example.com/atom/1"/>
            <summary>Atom summary</summary>
            <updated>2024-01-01T00:00:00Z</updated>
          </entry>
        </feed>"#;

        let feed = feed_rs::parser::parse(xml.as_bytes()).unwrap();
        assert_eq!(feed.entries.len(), 1);
        assert_eq!(feed.entries[0].title.as_ref().unwrap().content, "Atom Entry");
    }

    #[test]
    fn test_limit_applied() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item><title>1</title><link>https://example.com/1</link></item>
            <item><title>2</title><link>https://example.com/2</link></item>
            <item><title>3</title><link>https://example.com/3</link></item>
            <item><title>4</title><link>https://example.com/4</link></item>
            <item><title>5</title><link>https://example.com/5</link></item>
          </channel>
        </rss>"#;

        let feed = feed_rs::parser::parse(xml.as_bytes()).unwrap();
        let items: Vec<ContentItem> = feed
            .entries
            .into_iter()
            .take(3)
            .map(|entry| ContentItem {
                title: entry.title.map(|t| t.content).unwrap_or_default(),
                url: entry.links.first().map(|l| l.href.clone()).unwrap_or_default(),
                summary: String::new(),
                published: None,
                image_url: None,
            })
            .collect();

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].title, "1");
        assert_eq!(items[2].title, "3");
    }

    #[test]
    fn test_extract_og_image_double_quotes() {
        let html = r#"<html><head><meta property="og:image" content="https://example.com/img.jpg"/></head></html>"#;
        assert_eq!(
            extract_og_image_from_html(html),
            Some("https://example.com/img.jpg".to_string())
        );
    }

    #[test]
    fn test_extract_og_image_single_quotes() {
        let html = "<html><head><meta property='og:image' content='https://example.com/img.png'/></head></html>";
        assert_eq!(
            extract_og_image_from_html(html),
            Some("https://example.com/img.png".to_string())
        );
    }

    #[test]
    fn test_extract_og_image_skips_og_image_width() {
        let html = r#"<html><head><meta property="og:image:width" content="1200"/><meta property="og:image" content="https://example.com/real.jpg"/></head></html>"#;
        assert_eq!(
            extract_og_image_from_html(html),
            Some("https://example.com/real.jpg".to_string())
        );
    }

    #[test]
    fn test_extract_og_image_missing() {
        let html = r#"<html><head><title>No OG</title></head></html>"#;
        assert_eq!(extract_og_image_from_html(html), None);
    }

    #[test]
    fn test_extract_og_image_content_before_property() {
        let html = r#"<meta content="https://example.com/reversed.jpg" property="og:image"/>"#;
        assert_eq!(
            extract_og_image_from_html(html),
            Some("https://example.com/reversed.jpg".to_string())
        );
    }
}

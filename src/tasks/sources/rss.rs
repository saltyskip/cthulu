use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

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

    let items: Vec<ContentItem> = feed
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
            }
        })
        .collect();

    Ok(items)
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
            })
            .collect();

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].title, "1");
        assert_eq!(items[2].title, "3");
    }
}

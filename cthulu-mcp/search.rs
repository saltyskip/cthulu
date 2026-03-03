//! Web search client with two-tier strategy:
//!
//! PRIMARY  — SearXNG self-hosted JSON API (no rate limit, unlimited)
//!            GET http://localhost:8888/search?q=...&format=json
//!            Uses all engines configured in searxng-settings.yml (Bing, Brave, DDG).
//!
//! FALLBACK — DuckDuckGo HTML scrape (same logic as nickclyde/duckduckgo-mcp-server)
//!            POST https://html.duckduckgo.com/html
//!            Rate-limited to 30 searches/min and 20 fetches/min to avoid IP bans.
//!
//! fetch_content — Direct HTTP GET of any URL + HTML-to-text via scraper crate.
//!                 No rate limit needed (fetching target pages, not a search engine).

use crate::rate_limiter::RateLimiter;
use anyhow::{anyhow, Result};
use scraper::{Html, Selector};
use serde::Deserialize;

const DDG_URL: &str = "https://html.duckduckgo.com/html";
const DDG_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";

// ── SearXNG JSON response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SearxngResponse {
    results: Vec<SearxngResult>,
}

#[derive(Debug, Deserialize)]
struct SearxngResult {
    title: String,
    url: String,
    #[serde(default)]
    content: String,
}

// ── Public search result ──────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SearchResult {
    pub position: usize,
    pub title: String,
    pub url: String,
    pub snippet: String,
}

// ── SearchClient ──────────────────────────────────────────────────────────────

pub struct SearchClient {
    /// Base URL of the self-hosted SearXNG instance, e.g. "http://localhost:8888".
    /// None means skip SearXNG and go straight to DDG fallback.
    searxng_url: Option<String>,

    /// Rate limiter only engages on the DDG fallback search path.
    rl_search: RateLimiter,

    http: reqwest::Client,
}

impl SearchClient {
    pub fn new(searxng_url: Option<String>) -> Self {
        Self {
            searxng_url,
            rl_search: RateLimiter::new(30),
            http: reqwest::Client::builder()
                .user_agent(DDG_USER_AGENT)
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Search the web. Tries SearXNG first; falls back to DuckDuckGo.
    /// Returns a formatted string ready for LLM consumption.
    pub async fn search(&self, query: &str, max_results: usize) -> Result<String> {
        // Try SearXNG primary path
        if let Some(ref base) = self.searxng_url {
            match self.searxng_search(base, query, max_results).await {
                Ok(results) if !results.is_empty() => {
                    return Ok(format_results(&results, "SearXNG"));
                }
                Ok(_) => {
                    // Empty results — fall through to DDG
                }
                Err(e) => {
                    // SearXNG unreachable — fall through to DDG
                    eprintln!("[cthulu-mcp] SearXNG unavailable ({e}), falling back to DuckDuckGo");
                }
            }
        }

        // DDG fallback — apply rate limiter to protect against IP bans
        self.rl_search.acquire().await;
        let results = self.ddg_search(query, max_results).await?;
        Ok(format_results(&results, "DuckDuckGo (fallback)"))
    }

    /// Fetch and parse text content from any URL.
    /// Strips scripts, styles, nav, header, footer — returns clean readable text.
    /// Truncates at 8 000 chars (same as Python implementation).
    /// No rate limit — this fetches arbitrary target pages, not a search engine.
    pub async fn fetch_content(&self, url: &str) -> Result<String> {
        self.fetch_and_parse(url).await
    }

    // ── SearXNG primary ───────────────────────────────────────────────────────

    async fn searxng_search(
        &self,
        base: &str,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>> {
        let url = format!(
            "{}/search?q={}&format=json&pageno=1",
            base.trim_end_matches('/'),
            urlencoding::encode(query)
        );

        let resp = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<SearxngResponse>()
            .await?;

        let results = resp
            .results
            .into_iter()
            .take(max_results)
            .enumerate()
            .map(|(i, r)| SearchResult {
                position: i + 1,
                title: r.title,
                url: r.url,
                snippet: r.content,
            })
            .collect();

        Ok(results)
    }

    // ── DuckDuckGo fallback ───────────────────────────────────────────────────

    async fn ddg_search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>> {
        let params = [("q", query), ("b", ""), ("kl", "")];
        let resp = self
            .http
            .post(DDG_URL)
            .header("User-Agent", DDG_USER_AGENT)
            .form(&params)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        self.parse_ddg_html(&resp, max_results)
    }

    fn parse_ddg_html(&self, html: &str, max_results: usize) -> Result<Vec<SearchResult>> {
        let document = Html::parse_document(html);

        let result_sel = Selector::parse(".result").map_err(|e| anyhow!("{e}"))?;
        let title_sel = Selector::parse(".result__title").map_err(|e| anyhow!("{e}"))?;
        let snippet_sel = Selector::parse(".result__snippet").map_err(|e| anyhow!("{e}"))?;
        let a_sel = Selector::parse("a").map_err(|e| anyhow!("{e}"))?;

        let mut results = Vec::new();

        for node in document.select(&result_sel) {
            if results.len() >= max_results {
                break;
            }

            let title_node = match node.select(&title_sel).next() {
                Some(n) => n,
                None => continue,
            };

            let link_node = match title_node.select(&a_sel).next() {
                Some(n) => n,
                None => continue,
            };

            let title = link_node.text().collect::<String>().trim().to_string();
            let raw_href = link_node
                .value()
                .attr("href")
                .unwrap_or("")
                .to_string();

            // Skip ad results (same check as Python)
            if raw_href.contains("y.js") {
                continue;
            }

            // Clean DuckDuckGo redirect URLs
            let url = if raw_href.starts_with("//duckduckgo.com/l/?uddg=") {
                raw_href
                    .split("uddg=")
                    .nth(1)
                    .and_then(|s| s.split('&').next())
                    .map(|s| urlencoding::decode(s).unwrap_or_default().into_owned())
                    .unwrap_or(raw_href)
            } else {
                raw_href
            };

            let snippet = node
                .select(&snippet_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            results.push(SearchResult {
                position: results.len() + 1,
                title,
                url,
                snippet,
            });
        }

        Ok(results)
    }

    // ── Content fetching ──────────────────────────────────────────────────────

    async fn fetch_and_parse(&self, url: &str) -> Result<String> {
        let resp = self
            .http
            .get(url)
            .header("User-Agent", DDG_USER_AGENT)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let document = Html::parse_document(&resp);

        // Remove noise elements (same as Python)
        let noise_sel =
            Selector::parse("script, style, nav, header, footer").map_err(|e| anyhow!("{e}"))?;

        // Build clean text — scraper doesn't mutate, so we collect text from
        // all nodes that are NOT inside noise elements.
        let body_sel = Selector::parse("body").map_err(|e| anyhow!("{e}"))?;
        let body = document.select(&body_sel).next();

        let raw_text = if let Some(body) = body {
            // Walk all text nodes that aren't inside noise tags
            let noise_tags = ["script", "style", "nav", "header", "footer"];
            let mut parts = Vec::new();
            for node in body.descendants() {
                if let Some(text) = node.value().as_text() {
                    // Check ancestors — skip if any ancestor is a noise tag
                    let in_noise = node.ancestors().any(|a| {
                        a.value()
                            .as_element()
                            .map(|el| noise_tags.contains(&el.name()))
                            .unwrap_or(false)
                    });
                    if !in_noise {
                        let t = text.trim();
                        if !t.is_empty() {
                            parts.push(t.to_string());
                        }
                    }
                }
            }
            parts.join(" ")
        } else {
            // Fallback: strip all tags, collect text
            document
                .root_element()
                .text()
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        };

        // Normalise whitespace
        let text = raw_text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        // Truncate at ~8 000 chars (same as Python).
        // Use char_indices to find a safe UTF-8 boundary — slicing at an
        // arbitrary byte offset panics if it lands inside a multi-byte char.
        let text = if text.len() > 8000 {
            let end = text[..8000]
                .char_indices()
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(8000);
            format!("{}... [content truncated]", &text[..end])
        } else {
            text
        };

        // Remove noise selectors from document (we already skipped them above)
        let _ = noise_sel; // suppress unused warning

        Ok(text)
    }
}

// ── Formatting ────────────────────────────────────────────────────────────────

fn format_results(results: &[SearchResult], source: &str) -> String {
    if results.is_empty() {
        return format!(
            "No results found. Source: {source}. \
             DuckDuckGo may have detected bot traffic — try rephrasing or retry in a minute."
        );
    }

    let mut out = format!("Found {} result(s) via {source}:\n\n", results.len());
    for r in results {
        out.push_str(&format!(
            "{}. {}\n   URL: {}\n   Summary: {}\n\n",
            r.position, r.title, r.url, r.snippet
        ));
    }
    out
}

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

use super::Sink;

const NOTION_API_VERSION: &str = "2022-06-28";
const MAX_BLOCKS_PER_REQUEST: usize = 100;

pub struct NotionSink {
    http_client: Arc<reqwest::Client>,
    token: String,
    database_id: String,
}

impl NotionSink {
    pub fn new(http_client: Arc<reqwest::Client>, token: String, database_id: String) -> Self {
        Self {
            http_client,
            token,
            database_id,
        }
    }
}

#[async_trait]
impl Sink for NotionSink {
    async fn deliver(&self, text: &str) -> Result<()> {
        let blocks = markdown_to_notion_blocks(text);
        let title = extract_title(text);

        // First batch: create page with up to 100 blocks
        let (first_batch, remaining) = if blocks.len() > MAX_BLOCKS_PER_REQUEST {
            blocks.split_at(MAX_BLOCKS_PER_REQUEST)
        } else {
            (blocks.as_slice(), [].as_slice())
        };

        let body = json!({
            "parent": { "database_id": &self.database_id },
            "properties": {
                "Name": {
                    "title": [{
                        "text": { "content": title }
                    }]
                }
            },
            "children": first_batch,
        });

        let response = self
            .http_client
            .post("https://api.notion.com/v1/pages")
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .json(&body)
            .send()
            .await
            .context("failed to create Notion page")?;

        let status = response.status();
        let resp_body: Value = response
            .json()
            .await
            .context("failed to parse Notion API response")?;

        if !status.is_success() {
            let msg = resp_body["message"]
                .as_str()
                .unwrap_or("unknown error");
            anyhow::bail!("Notion API returned {status}: {msg}");
        }

        let page_id = resp_body["id"]
            .as_str()
            .context("Notion response missing page id")?;

        // Append remaining blocks in chunks
        for chunk in remaining.chunks(MAX_BLOCKS_PER_REQUEST) {
            let append_body = json!({ "children": chunk });

            let resp = self
                .http_client
                .patch(format!(
                    "https://api.notion.com/v1/blocks/{page_id}/children"
                ))
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Notion-Version", NOTION_API_VERSION)
                .json(&append_body)
                .send()
                .await
                .context("failed to append blocks to Notion page")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body: Value = resp.json().await.unwrap_or_default();
                let msg = body["message"].as_str().unwrap_or("unknown error");
                anyhow::bail!("Notion append blocks returned {status}: {msg}");
            }
        }

        tracing::info!("Delivered message to Notion");
        Ok(())
    }
}

fn extract_title(text: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("## ") {
            return heading.trim().to_string();
        }
        if let Some(heading) = trimmed.strip_prefix("# ") {
            return heading.trim().to_string();
        }
    }
    chrono::Utc::now().format("%Y-%m-%d Brief").to_string()
}

// ---------------------------------------------------------------------------
// Markdown → Notion blocks converter
// ---------------------------------------------------------------------------

fn markdown_to_notion_blocks(text: &str) -> Vec<Value> {
    let mut blocks = Vec::new();
    let mut paragraph_lines: Vec<&str> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            blocks.push(json!({ "object": "block", "type": "divider", "divider": {} }));
            continue;
        }

        // Headings
        if let Some(rest) = trimmed.strip_prefix("### ") {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            blocks.push(heading_block("heading_3", rest.trim()));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            blocks.push(heading_block("heading_2", rest.trim()));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            blocks.push(heading_block("heading_1", rest.trim()));
            continue;
        }

        // Bulleted list item
        if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            blocks.push(json!({
                "object": "block",
                "type": "bulleted_list_item",
                "bulleted_list_item": {
                    "rich_text": parse_inline(rest.trim()),
                }
            }));
            continue;
        }

        // Empty line flushes paragraph
        if trimmed.is_empty() {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            continue;
        }

        // Accumulate paragraph text
        paragraph_lines.push(trimmed);
    }

    flush_paragraph(&mut paragraph_lines, &mut blocks);
    blocks
}

fn flush_paragraph(lines: &mut Vec<&str>, blocks: &mut Vec<Value>) {
    if lines.is_empty() {
        return;
    }
    let text = lines.join(" ");
    blocks.push(json!({
        "object": "block",
        "type": "paragraph",
        "paragraph": {
            "rich_text": parse_inline(&text),
        }
    }));
    lines.clear();
}

fn heading_block(kind: &str, text: &str) -> Value {
    json!({
        "object": "block",
        "type": kind,
        kind: {
            "rich_text": parse_inline(text),
        }
    })
}

/// Parse inline markdown: **bold**, `code`, [text](url)
fn parse_inline(text: &str) -> Vec<Value> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the next special token
        let next_bold = remaining.find("**");
        let next_code = remaining.find('`');
        let next_link = remaining.find('[');

        let earliest = [next_bold, next_code, next_link]
            .into_iter()
            .flatten()
            .min();

        let Some(pos) = earliest else {
            // No more special tokens, emit the rest as plain text
            if !remaining.is_empty() {
                spans.push(rich_text_plain(remaining));
            }
            break;
        };

        // Emit plain text before the token
        if pos > 0 {
            spans.push(rich_text_plain(&remaining[..pos]));
        }

        let after = &remaining[pos..];

        // Bold: **...**
        if after.starts_with("**") {
            if let Some(end) = after[2..].find("**") {
                let bold_text = &after[2..2 + end];
                spans.push(json!({
                    "type": "text",
                    "text": { "content": bold_text },
                    "annotations": { "bold": true },
                }));
                remaining = &after[2 + end + 2..];
                continue;
            }
        }

        // Code: `...`
        if after.starts_with('`') {
            if let Some(end) = after[1..].find('`') {
                let code_text = &after[1..1 + end];
                spans.push(json!({
                    "type": "text",
                    "text": { "content": code_text },
                    "annotations": { "code": true },
                }));
                remaining = &after[1 + end + 1..];
                continue;
            }
        }

        // Link: [text](url)
        if after.starts_with('[') {
            if let Some(close_bracket) = after.find("](") {
                let link_text = &after[1..close_bracket];
                let url_start = close_bracket + 2;
                if let Some(close_paren) = after[url_start..].find(')') {
                    let url = &after[url_start..url_start + close_paren];
                    spans.push(json!({
                        "type": "text",
                        "text": {
                            "content": link_text,
                            "link": { "url": url },
                        },
                    }));
                    remaining = &after[url_start + close_paren + 1..];
                    continue;
                }
            }
        }

        // Token didn't match a complete pattern — emit the character as plain text
        spans.push(rich_text_plain(&after[..1]));
        remaining = &after[1..];
    }

    if spans.is_empty() {
        spans.push(rich_text_plain(""));
    }

    spans
}

fn rich_text_plain(text: &str) -> Value {
    json!({
        "type": "text",
        "text": { "content": text },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title_h2() {
        assert_eq!(extract_title("some text\n## My Title\nmore"), "My Title");
    }

    #[test]
    fn test_extract_title_h1() {
        assert_eq!(extract_title("# Top Heading\nrest"), "Top Heading");
    }

    #[test]
    fn test_extract_title_fallback() {
        let title = extract_title("no headings here");
        assert!(title.contains("Brief"));
    }

    #[test]
    fn test_blocks_heading() {
        let blocks = markdown_to_notion_blocks("# H1\n## H2\n### H3");
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0]["type"], "heading_1");
        assert_eq!(blocks[1]["type"], "heading_2");
        assert_eq!(blocks[2]["type"], "heading_3");
    }

    #[test]
    fn test_blocks_paragraph() {
        let blocks = markdown_to_notion_blocks("Hello world");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "paragraph");
    }

    #[test]
    fn test_blocks_bullet() {
        let blocks = markdown_to_notion_blocks("- item one\n- item two");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "bulleted_list_item");
        assert_eq!(blocks[1]["type"], "bulleted_list_item");
    }

    #[test]
    fn test_blocks_divider() {
        let blocks = markdown_to_notion_blocks("above\n\n---\n\nbelow");
        assert_eq!(blocks[1]["type"], "divider");
    }

    #[test]
    fn test_inline_bold() {
        let spans = parse_inline("hello **world**");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0]["text"]["content"], "hello ");
        assert_eq!(spans[1]["text"]["content"], "world");
        assert_eq!(spans[1]["annotations"]["bold"], true);
    }

    #[test]
    fn test_inline_code() {
        let spans = parse_inline("use `foo` here");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1]["text"]["content"], "foo");
        assert_eq!(spans[1]["annotations"]["code"], true);
    }

    #[test]
    fn test_inline_link() {
        let spans = parse_inline("click [here](https://example.com) now");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1]["text"]["content"], "here");
        assert_eq!(spans[1]["text"]["link"]["url"], "https://example.com");
    }

    #[test]
    fn test_mixed_content() {
        let md = "# Title\n\nSome **bold** text.\n\n- item\n\n---\n\nEnd.";
        let blocks = markdown_to_notion_blocks(md);
        assert_eq!(blocks[0]["type"], "heading_1");
        assert_eq!(blocks[1]["type"], "paragraph");
        assert_eq!(blocks[2]["type"], "bulleted_list_item");
        assert_eq!(blocks[3]["type"], "divider");
        assert_eq!(blocks[4]["type"], "paragraph");
    }
}

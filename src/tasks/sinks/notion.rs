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

        // Image: ![caption](url)
        if let Some((caption, url)) = parse_image_markdown(trimmed) {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            let mut block = json!({
                "object": "block",
                "type": "image",
                "image": {
                    "type": "external",
                    "external": { "url": url },
                }
            });
            if !caption.is_empty() {
                block["image"]["caption"] = json!([{
                    "type": "text",
                    "text": { "content": caption },
                }]);
            }
            blocks.push(block);
            continue;
        }

        // Meme: [meme:template|top|bottom]
        if let Some(url) = parse_meme_marker(trimmed) {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            blocks.push(json!({
                "object": "block",
                "type": "image",
                "image": {
                    "type": "external",
                    "external": { "url": url },
                }
            }));
            continue;
        }

        // Callout: > 🔥 text (blockquote where first char after > is emoji)
        if let Some(rest) = trimmed.strip_prefix("> ") {
            let mut chars = rest.chars();
            if let Some(first_char) = chars.next() {
                if is_likely_emoji(first_char) {
                    // Consume trailing variation selector (U+FE0F) if present
                    let mut emoji = first_char.to_string();
                    let remaining = chars.as_str();
                    let body = if remaining.starts_with('\u{FE0F}') {
                        emoji.push('\u{FE0F}');
                        chars.next();
                        chars.as_str().trim()
                    } else {
                        remaining.trim()
                    };
                    flush_paragraph(&mut paragraph_lines, &mut blocks);
                    blocks.push(json!({
                        "object": "block",
                        "type": "callout",
                        "callout": {
                            "rich_text": parse_inline(body),
                            "icon": { "type": "emoji", "emoji": emoji },
                        }
                    }));
                    continue;
                }
            }

            // Plain blockquote → quote block
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            blocks.push(json!({
                "object": "block",
                "type": "quote",
                "quote": {
                    "rich_text": parse_inline(rest),
                }
            }));
            continue;
        }

        // Bookmark: bare URL on its own line
        if trimmed.starts_with("https://") && !trimmed.contains(' ') {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            blocks.push(json!({
                "object": "block",
                "type": "bookmark",
                "bookmark": { "url": trimmed },
            }));
            continue;
        }

        // Bookmark: [Title](url) alone on a line (not an image)
        if trimmed.starts_with('[') && !trimmed.starts_with("[meme:") {
            if let Some((link_text, url)) = parse_link_only(trimmed) {
                flush_paragraph(&mut paragraph_lines, &mut blocks);
                let mut block = json!({
                    "object": "block",
                    "type": "bookmark",
                    "bookmark": { "url": url },
                });
                if !link_text.is_empty() {
                    block["bookmark"]["caption"] = json!([{
                        "type": "text",
                        "text": { "content": link_text },
                    }]);
                }
                blocks.push(block);
                continue;
            }
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

fn is_likely_emoji(c: char) -> bool {
    matches!(c as u32,
        0x2600..=0x27BF |   // Misc symbols, dingbats
        0x1F300..=0x1F9FF | // Main emoji blocks
        0x1FA00..=0x1FAFF | // Extended-A
        0xFE00..=0xFE0F     // Variation selectors
    )
}

fn parse_image_markdown(line: &str) -> Option<(&str, &str)> {
    let line = line.strip_prefix("![")?;
    let close_bracket = line.find("](")?;
    let caption = &line[..close_bracket];
    let rest = &line[close_bracket + 2..];
    let close_paren = rest.find(')')?;
    let url = &rest[..close_paren];
    // Ensure nothing follows the closing paren (it's the whole line)
    if rest[close_paren + 1..].trim().is_empty() && !url.is_empty() {
        Some((caption, url))
    } else {
        None
    }
}

fn parse_meme_marker(line: &str) -> Option<String> {
    let inner = line.strip_prefix("[meme:")?.strip_suffix(']')?;
    let parts: Vec<&str> = inner.splitn(3, '|').collect();
    if parts.len() < 2 {
        return None;
    }
    let template = parts[0].trim();
    let top = encode_meme_text(parts.get(1).unwrap_or(&"").trim());
    let bottom = if parts.len() == 3 {
        encode_meme_text(parts[2].trim())
    } else {
        "_".to_string()
    };
    Some(format!(
        "https://api.memegen.link/images/{template}/{top}/{bottom}.png"
    ))
}

fn encode_meme_text(text: &str) -> String {
    if text.is_empty() {
        return "_".to_string();
    }
    text.replace('%', "~p")
        .replace('#', "~h")
        .replace('/', "~s")
        .replace('?', "~q")
        .replace('$', "~d")
        .replace('"', "''")
        .replace('&', "~a")
        .replace(' ', "_")
}

fn parse_link_only(line: &str) -> Option<(&str, &str)> {
    // Match [text](url) where it's the entire line
    let line = line.strip_prefix('[')?;
    let close_bracket = line.find("](")?;
    let text = &line[..close_bracket];
    let rest = &line[close_bracket + 2..];
    let close_paren = rest.find(')')?;
    let url = &rest[..close_paren];
    // Reject empty link text — let it fall through to paragraph handling
    if rest[close_paren + 1..].trim().is_empty() && !url.is_empty() && !text.is_empty() {
        Some((text, url))
    } else {
        None
    }
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

const NOTION_MAX_TEXT_LENGTH: usize = 2000;

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

    chunk_rich_text(spans)
}

/// Split any rich_text span exceeding Notion's 2000-char limit into multiple
/// spans, preserving annotations and links.
fn chunk_rich_text(spans: Vec<Value>) -> Vec<Value> {
    let mut out = Vec::with_capacity(spans.len());
    for span in spans {
        let content = span["text"]["content"].as_str().unwrap_or("");
        if content.len() <= NOTION_MAX_TEXT_LENGTH {
            out.push(span);
            continue;
        }

        // Clone annotations / link from the original span
        let annotations = span.get("annotations").cloned();
        let link = span["text"].get("link").cloned();

        let mut remaining = content;
        while !remaining.is_empty() {
            let mut end = remaining.len().min(NOTION_MAX_TEXT_LENGTH);
            while end < remaining.len() && !remaining.is_char_boundary(end) {
                end -= 1;
            }
            let chunk = &remaining[..end];
            remaining = &remaining[end..];

            let mut text_obj = json!({ "content": chunk });
            if let Some(l) = &link {
                text_obj["link"] = l.clone();
            }
            let mut s = json!({ "type": "text", "text": text_obj });
            if let Some(a) = &annotations {
                s["annotations"] = a.clone();
            }
            out.push(s);
        }
    }
    out
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
    fn test_long_text_chunked() {
        let long = "a".repeat(4500);
        let spans = parse_inline(&long);
        assert!(spans.len() >= 3);
        for span in &spans {
            let content = span["text"]["content"].as_str().unwrap();
            assert!(content.len() <= NOTION_MAX_TEXT_LENGTH);
        }
        // Total content preserved
        let total: usize = spans.iter().map(|s| s["text"]["content"].as_str().unwrap().len()).sum();
        assert_eq!(total, 4500);
    }

    #[test]
    fn test_long_bold_chunked_preserves_annotations() {
        let long = format!("**{}**", "b".repeat(3000));
        let spans = parse_inline(&long);
        assert!(spans.len() >= 2);
        for span in &spans {
            assert_eq!(span["annotations"]["bold"], true);
            assert!(span["text"]["content"].as_str().unwrap().len() <= NOTION_MAX_TEXT_LENGTH);
        }
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

    #[test]
    fn test_image_block() {
        let blocks = markdown_to_notion_blocks("![banner](https://example.com/img.jpg)");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["image"]["external"]["url"], "https://example.com/img.jpg");
        assert_eq!(blocks[0]["image"]["caption"][0]["text"]["content"], "banner");
    }

    #[test]
    fn test_image_no_caption() {
        let blocks = markdown_to_notion_blocks("![](https://example.com/img.jpg)");
        assert_eq!(blocks[0]["type"], "image");
        assert!(blocks[0]["image"]["caption"].is_null());
    }

    #[test]
    fn test_meme_block() {
        let blocks = markdown_to_notion_blocks("[meme:drake|fiat money|bitcoin]");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(
            blocks[0]["image"]["external"]["url"],
            "https://api.memegen.link/images/drake/fiat_money/bitcoin.png"
        );
    }

    #[test]
    fn test_meme_special_chars() {
        assert_eq!(encode_meme_text("why not?"), "why_not~q");
        assert_eq!(encode_meme_text("50% off"), "50~p_off");
        assert_eq!(encode_meme_text("a/b"), "a~sb");
        assert_eq!(encode_meme_text("tag #1"), "tag_~h1");
        assert_eq!(encode_meme_text("$650M"), "~d650M");
        assert_eq!(encode_meme_text(r#"the "gloom""#), "the_''gloom''");
        assert_eq!(encode_meme_text("A & B"), "A_~a_B");
    }

    #[test]
    fn test_callout_block() {
        let blocks = markdown_to_notion_blocks("> \u{1f525} This is hot news");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "callout");
        assert_eq!(blocks[0]["callout"]["icon"]["emoji"], "\u{1f525}");
    }

    #[test]
    fn test_quote_block() {
        let blocks = markdown_to_notion_blocks("> Some quoted text");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "quote");
    }

    #[test]
    fn test_bookmark_bare_url() {
        let blocks = markdown_to_notion_blocks("https://example.com/article");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "bookmark");
        assert_eq!(blocks[0]["bookmark"]["url"], "https://example.com/article");
    }

    #[test]
    fn test_bookmark_link_syntax() {
        let blocks = markdown_to_notion_blocks("[Read More](https://example.com/article)");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "bookmark");
        assert_eq!(blocks[0]["bookmark"]["url"], "https://example.com/article");
        assert_eq!(blocks[0]["bookmark"]["caption"][0]["text"]["content"], "Read More");
    }

    #[test]
    fn test_empty_link_text_falls_through_to_paragraph() {
        let blocks = markdown_to_notion_blocks("[](https://example.com/article)");
        assert_eq!(blocks.len(), 1);
        // Empty link text should NOT create a bookmark — falls through to paragraph
        assert_eq!(blocks[0]["type"], "paragraph");
    }

    #[test]
    fn test_all_new_block_types() {
        let md = "\
![banner](https://img.com/banner.jpg)

# Newsletter

> \u{1f4a1} Key insight here

> Plain quote

https://example.com/source

[Source](https://example.com/linked)

[meme:buzz|rich notion pages|rich notion pages everywhere]";

        let blocks = markdown_to_notion_blocks(md);
        assert_eq!(blocks[0]["type"], "image");     // banner
        assert_eq!(blocks[1]["type"], "heading_1");  // # Newsletter
        assert_eq!(blocks[2]["type"], "callout");    // > 💡
        assert_eq!(blocks[3]["type"], "quote");      // > Plain
        assert_eq!(blocks[4]["type"], "bookmark");   // bare URL
        assert_eq!(blocks[5]["type"], "bookmark");   // [Source](url)
        assert_eq!(blocks[6]["type"], "image");      // meme
    }
}

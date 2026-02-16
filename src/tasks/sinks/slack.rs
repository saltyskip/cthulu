use anyhow::{Context as AnyhowContext, Result};
use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;
use serde_json::json;

// ---------------------------------------------------------------------------
// Block Kit types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Block {
    Header { text: TextObject },
    Section { text: TextObject },
    Context { elements: Vec<ContextElement> },
    RichText { elements: Vec<RichTextElement> },
    Divider,
}

impl Serialize for Block {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Block::Header { text } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "header")?;
                map.serialize_entry("text", text)?;
                map.end()
            }
            Block::Section { text } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "section")?;
                map.serialize_entry("text", text)?;
                map.end()
            }
            Block::Context { elements } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "context")?;
                map.serialize_entry("elements", elements)?;
                map.end()
            }
            Block::RichText { elements } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "rich_text")?;
                map.serialize_entry("elements", elements)?;
                map.end()
            }
            Block::Divider => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "divider")?;
                map.end()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TextObject {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
}

// -- Context block elements --

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextElement {
    #[serde(rename = "mrkdwn")]
    Mrkdwn { text: String },
}

// -- Rich text block elements --

#[derive(Debug, Clone)]
pub enum RichTextElement {
    Section { elements: Vec<RichTextInline> },
    List { style: ListStyle, elements: Vec<RichTextListItem> },
}

impl Serialize for RichTextElement {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            RichTextElement::Section { elements } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "rich_text_section")?;
                map.serialize_entry("elements", elements)?;
                map.end()
            }
            RichTextElement::List { style, elements } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "rich_text_list")?;
                map.serialize_entry("style", match style {
                    ListStyle::Bullet => "bullet",
                    ListStyle::Ordered => "ordered",
                })?;
                map.serialize_entry("elements", elements)?;
                map.end()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ListStyle {
    Bullet,
    Ordered,
}

#[derive(Debug, Clone)]
pub struct RichTextListItem {
    pub elements: Vec<RichTextInline>,
}

impl Serialize for RichTextListItem {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("type", "rich_text_section")?;
        map.serialize_entry("elements", &self.elements)?;
        map.end()
    }
}

#[derive(Debug, Clone)]
pub enum RichTextInline {
    Text { text: String, style: Option<RichTextStyle> },
    Link { url: String, text: Option<String>, style: Option<RichTextStyle> },
    Emoji { name: String },
}

impl Serialize for RichTextInline {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            RichTextInline::Text { text, style } => {
                let has_style = style.as_ref().is_some_and(|s| s.has_any());
                let count = 2 + usize::from(has_style);
                let mut map = serializer.serialize_map(Some(count))?;
                map.serialize_entry("type", "text")?;
                map.serialize_entry("text", text)?;
                if let Some(s) = style {
                    if s.has_any() {
                        map.serialize_entry("style", s)?;
                    }
                }
                map.end()
            }
            RichTextInline::Link { url, text, style } => {
                let has_style = style.as_ref().is_some_and(|s| s.has_any());
                let mut count = 2;
                if text.is_some() { count += 1; }
                if has_style { count += 1; }
                let mut map = serializer.serialize_map(Some(count))?;
                map.serialize_entry("type", "link")?;
                map.serialize_entry("url", url)?;
                if let Some(t) = text {
                    map.serialize_entry("text", t)?;
                }
                if let Some(s) = style {
                    if s.has_any() {
                        map.serialize_entry("style", s)?;
                    }
                }
                map.end()
            }
            RichTextInline::Emoji { name } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "emoji")?;
                map.serialize_entry("name", name)?;
                map.end()
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RichTextStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

impl RichTextStyle {
    fn has_any(&self) -> bool {
        self.bold || self.italic || self.code
    }
}

impl Serialize for RichTextStyle {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let count = usize::from(self.bold) + usize::from(self.italic) + usize::from(self.code);
        let mut map = serializer.serialize_map(Some(count))?;
        if self.bold {
            map.serialize_entry("bold", &true)?;
        }
        if self.italic {
            map.serialize_entry("italic", &true)?;
        }
        if self.code {
            map.serialize_entry("code", &true)?;
        }
        map.end()
    }
}

const MAX_HEADER_LEN: usize = 150;
const MAX_SECTION_LEN: usize = 3000;
const MAX_BLOCKS_PER_MESSAGE: usize = 50;

// ---------------------------------------------------------------------------
// Webhook (legacy) path — unchanged
// ---------------------------------------------------------------------------

pub async fn post_message(
    client: &reqwest::Client,
    webhook_url_env: &str,
    text: &str,
) -> Result<()> {
    let webhook_url = std::env::var(webhook_url_env)
        .with_context(|| format!("environment variable {webhook_url_env} not set"))?;

    post_to_url(client, &webhook_url, text).await
}

pub async fn post_to_url(
    client: &reqwest::Client,
    webhook_url: &str,
    text: &str,
) -> Result<()> {
    let slack_text = markdown_to_slack(text);

    let response = client
        .post(webhook_url)
        .json(&json!({ "text": slack_text }))
        .send()
        .await
        .context("failed to post to Slack webhook")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Slack webhook returned {status}: {body}");
    }

    tracing::info!("Delivered message to Slack");
    Ok(())
}

// ---------------------------------------------------------------------------
// Web API (Block Kit + threading) path
// ---------------------------------------------------------------------------

/// Post a message with Block Kit formatting and optional threading.
///
/// If `full_text` contains a `---THREAD---` delimiter, the part above becomes
/// the main channel message and the part below is posted as a thread reply.
pub async fn post_threaded_blocks(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    full_text: &str,
) -> Result<()> {
    let parts: Vec<&str> = full_text.splitn(2, "---THREAD---").collect();

    let main_text = parts[0].trim();
    let thread_text = parts.get(1).map(|s| s.trim());

    let main_blocks = markdown_to_blocks(main_text);
    let ts = post_blocks(client, bot_token, channel, &main_blocks, None)
        .await
        .context("failed to post main message")?;

    if let Some(detail) = thread_text {
        if !detail.is_empty() {
            let thread_blocks = markdown_to_blocks(detail);
            post_blocks(client, bot_token, channel, &thread_blocks, Some(&ts))
                .await
                .context("failed to post thread reply")?;
        }
    }

    tracing::info!("Delivered Block Kit message to Slack");
    Ok(())
}

/// Post blocks to Slack via `chat.postMessage`. Returns the message `ts`.
async fn post_blocks(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    blocks: &[Block],
    thread_ts: Option<&str>,
) -> Result<String> {
    let blocks = if blocks.len() > MAX_BLOCKS_PER_MESSAGE {
        let mut truncated = blocks[..MAX_BLOCKS_PER_MESSAGE - 1].to_vec();
        truncated.push(Block::Section {
            text: TextObject {
                kind: "mrkdwn",
                text: "_Message truncated — too many blocks._".to_string(),
            },
        });
        truncated
    } else {
        blocks.to_vec()
    };

    // Build a fallback plain-text summary from all text-bearing blocks
    let fallback: String = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Header { text } | Block::Section { text } => Some(text.text.clone()),
            Block::Context { elements } => {
                let parts: Vec<&str> = elements
                    .iter()
                    .map(|e| match e {
                        ContextElement::Mrkdwn { text } => text.as_str(),
                    })
                    .collect();
                Some(parts.join(" "))
            }
            Block::RichText { elements } => {
                let mut parts = Vec::new();
                for el in elements {
                    match el {
                        RichTextElement::Section { elements: inlines } => {
                            parts.push(extract_inline_text(inlines));
                        }
                        RichTextElement::List { elements: items, .. } => {
                            for item in items {
                                parts.push(format!("• {}", extract_inline_text(&item.elements)));
                            }
                        }
                    }
                }
                Some(parts.join("\n"))
            }
            Block::Divider => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut body = json!({
        "channel": channel,
        "blocks": blocks,
        "text": fallback,
    });

    if let Some(ts) = thread_ts {
        body["thread_ts"] = json!(ts);
    }

    let response = client
        .post("https://slack.com/api/chat.postMessage")
        .header("Authorization", format!("Bearer {bot_token}"))
        .json(&body)
        .send()
        .await
        .context("failed to call chat.postMessage")?;

    let status = response.status();
    let resp_body: serde_json::Value = response
        .json()
        .await
        .context("failed to parse Slack API response")?;

    if !status.is_success() || resp_body["ok"].as_bool() != Some(true) {
        let err = resp_body["error"].as_str().unwrap_or("unknown error");
        anyhow::bail!("chat.postMessage failed ({status}): {err}");
    }

    resp_body["ts"]
        .as_str()
        .map(|s| s.to_string())
        .context("Slack response missing ts field")
}

/// Extract plain text from a slice of rich text inlines.
fn extract_inline_text(inlines: &[RichTextInline]) -> String {
    inlines
        .iter()
        .map(|i| match i {
            RichTextInline::Text { text, .. } => text.clone(),
            RichTextInline::Link { text, url, .. } => {
                text.clone().unwrap_or_else(|| url.clone())
            }
            RichTextInline::Emoji { name } => format!(":{name}:"),
        })
        .collect::<Vec<_>>()
        .join("")
}

// ---------------------------------------------------------------------------
// Markdown → Block Kit blocks
// ---------------------------------------------------------------------------

/// Convert markdown text into Slack Block Kit blocks.
pub fn markdown_to_blocks(text: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut paragraph_lines: Vec<String> = Vec::new();
    let mut bullet_items: Vec<Vec<RichTextInline>> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Horizontal rule → flush + Divider
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            flush_bullets(&mut blocks, &mut bullet_items);
            blocks.push(Block::Divider);
            continue;
        }

        // # Header → Header block with auto emoji
        if let Some(header_text) = trimmed.strip_prefix("# ") {
            if !trimmed.starts_with("## ") {
                flush_paragraph(&mut blocks, &mut paragraph_lines);
                flush_bullets(&mut blocks, &mut bullet_items);
                let mut h = header_text.trim().to_string();
                h = maybe_prefix_emoji(&h);
                if h.len() > MAX_HEADER_LEN {
                    let mut end = MAX_HEADER_LEN;
                    while !h.is_char_boundary(end) {
                        end -= 1;
                    }
                    h.truncate(end);
                }
                blocks.push(Block::Header {
                    text: TextObject {
                        kind: "plain_text",
                        text: h,
                    },
                });
                continue;
            }
        }

        // ## or ### → bold mrkdwn Section block
        if let Some(header_text) = trimmed
            .strip_prefix("### ")
            .or_else(|| trimmed.strip_prefix("## "))
        {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            flush_bullets(&mut blocks, &mut bullet_items);
            let h = header_text.trim();
            blocks.push(Block::Section {
                text: TextObject {
                    kind: "mrkdwn",
                    text: format!("*{h}*"),
                },
            });
            continue;
        }

        // Bullet items → accumulate for RichText
        if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            bullet_items.push(parse_inline_elements(rest));
            continue;
        }

        // Empty line → flush both
        if trimmed.is_empty() {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            flush_bullets(&mut blocks, &mut bullet_items);
            continue;
        }

        // Metadata line → Context block
        if is_metadata_line(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            flush_bullets(&mut blocks, &mut bullet_items);
            blocks.push(Block::Context {
                elements: vec![ContextElement::Mrkdwn {
                    text: convert_bold(&convert_links(trimmed)),
                }],
            });
            continue;
        }

        // Everything else: accumulate as mrkdwn paragraph
        flush_bullets(&mut blocks, &mut bullet_items);
        paragraph_lines.push(line.to_string());
    }

    flush_paragraph(&mut blocks, &mut paragraph_lines);
    flush_bullets(&mut blocks, &mut bullet_items);
    blocks
}

/// Flush accumulated paragraph lines into Section blocks (chunked at 3000 chars).
fn flush_paragraph(blocks: &mut Vec<Block>, lines: &mut Vec<String>) {
    if lines.is_empty() {
        return;
    }

    let joined = lines.join("\n");
    lines.clear();

    let formatted = convert_bold(&convert_links(&joined));

    let mut chunk = String::new();
    for line in formatted.lines() {
        if !chunk.is_empty() && chunk.len() + 1 + line.len() > MAX_SECTION_LEN {
            push_section_block(blocks, &chunk);
            chunk.clear();
        }
        if !chunk.is_empty() {
            chunk.push('\n');
        }
        chunk.push_str(line);
    }

    if !chunk.is_empty() {
        push_section_block(blocks, &chunk);
    }
}

fn push_section_block(blocks: &mut Vec<Block>, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    blocks.push(Block::Section {
        text: TextObject {
            kind: "mrkdwn",
            text: trimmed.to_string(),
        },
    });
}

/// Flush accumulated bullet items into a RichText block with a bullet list.
fn flush_bullets(blocks: &mut Vec<Block>, items: &mut Vec<Vec<RichTextInline>>) {
    if items.is_empty() {
        return;
    }

    let list_items: Vec<RichTextListItem> = items
        .drain(..)
        .map(|elements| RichTextListItem { elements })
        .collect();

    blocks.push(Block::RichText {
        elements: vec![RichTextElement::List {
            style: ListStyle::Bullet,
            elements: list_items,
        }],
    });
}

// ---------------------------------------------------------------------------
// Inline element parser for rich text
// ---------------------------------------------------------------------------

/// Parse inline markdown into rich text elements.
/// Handles: **bold**, `code`, [text](url), :emoji:, and plain text.
fn parse_inline_elements(text: &str) -> Vec<RichTextInline> {
    let mut elements: Vec<RichTextInline> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut plain = String::new();

    while i < chars.len() {
        // **bold**
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_closing_double_star(&chars, i + 2) {
                if !plain.is_empty() {
                    elements.push(RichTextInline::Text {
                        text: std::mem::take(&mut plain),
                        style: None,
                    });
                }
                let bold_text: String = chars[i + 2..end].iter().collect();
                elements.push(RichTextInline::Text {
                    text: bold_text,
                    style: Some(RichTextStyle { bold: true, ..Default::default() }),
                });
                i = end + 2;
                continue;
            }
        }

        // `code`
        if chars[i] == '`' {
            if let Some(end) = find_closing_char(&chars, i + 1, '`') {
                if !plain.is_empty() {
                    elements.push(RichTextInline::Text {
                        text: std::mem::take(&mut plain),
                        style: None,
                    });
                }
                let code_text: String = chars[i + 1..end].iter().collect();
                elements.push(RichTextInline::Text {
                    text: code_text,
                    style: Some(RichTextStyle { code: true, ..Default::default() }),
                });
                i = end + 1;
                continue;
            }
        }

        // [text](url)
        if chars[i] == '[' {
            if let Some((link_text, url, end)) = parse_md_link(&chars, i) {
                if !plain.is_empty() {
                    elements.push(RichTextInline::Text {
                        text: std::mem::take(&mut plain),
                        style: None,
                    });
                }
                elements.push(RichTextInline::Link {
                    url,
                    text: Some(link_text),
                    style: None,
                });
                i = end;
                continue;
            }
        }

        // :emoji:
        if chars[i] == ':' {
            if let Some((name, end)) = parse_emoji(&chars, i) {
                if !plain.is_empty() {
                    elements.push(RichTextInline::Text {
                        text: std::mem::take(&mut plain),
                        style: None,
                    });
                }
                elements.push(RichTextInline::Emoji { name });
                i = end;
                continue;
            }
        }

        plain.push(chars[i]);
        i += 1;
    }

    if !plain.is_empty() {
        elements.push(RichTextInline::Text { text: plain, style: None });
    }

    if elements.is_empty() {
        elements.push(RichTextInline::Text { text: String::new(), style: None });
    }

    elements
}

/// Find closing character (e.g., backtick) starting from `start`.
fn find_closing_char(chars: &[char], start: usize, ch: char) -> Option<usize> {
    for i in start..chars.len() {
        if chars[i] == ch {
            return Some(i);
        }
    }
    None
}

/// Try to parse :emoji_name: starting at position `start`.
fn parse_emoji(chars: &[char], start: usize) -> Option<(String, usize)> {
    if start + 2 >= chars.len() {
        return None;
    }
    let mut name = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == ':' {
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '+') {
                return Some((name, i + 1));
            }
            return None;
        }
        if chars[i] == ' ' || chars[i] == '\n' {
            return None;
        }
        name.push(chars[i]);
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Header auto-emoji
// ---------------------------------------------------------------------------

/// Add an emoji prefix to a header if it matches known keywords.
fn maybe_prefix_emoji(header: &str) -> String {
    let lower = header.to_lowercase();

    let emoji = if lower.contains("what shipped") || lower.contains("shipped") {
        ":ship:"
    } else if lower.contains("changelog") || lower.contains("update") {
        ":memo:"
    } else if lower.contains("coming soon") || lower.contains("upcoming") {
        ":crystal_ball:"
    } else if lower.contains("breaking") || lower.contains("warning") {
        ":warning:"
    } else if lower.contains("fix") || lower.contains("bug") {
        ":bug:"
    } else if lower.contains("performance") || lower.contains("speed") {
        ":zap:"
    } else if lower.contains("note") {
        ":memo:"
    } else {
        return header.to_string();
    };

    // Don't double-prefix if there's already an emoji-like pattern at the start
    if header.starts_with(':') {
        return header.to_string();
    }

    format!("{emoji} {header}")
}

// ---------------------------------------------------------------------------
// Metadata line detection
// ---------------------------------------------------------------------------

/// Detect summary/stats lines that should render as Context blocks.
fn is_metadata_line(line: &str) -> bool {
    let lower = line.to_lowercase();

    // Lines like "5 PRs merged across 3 repos" or "8 improvements shipped"
    let has_leading_number = lower.chars().next().is_some_and(|c| c.is_ascii_digit());
    let stats_keywords = ["total", "merged", "shipped", "improvement", "change", "update", "repo", "across"];
    let has_stats_keyword = stats_keywords.iter().any(|kw| lower.contains(kw));

    has_leading_number && has_stats_keyword
}

// ---------------------------------------------------------------------------
// Markdown → Slack mrkdwn (plain text for webhooks)
// ---------------------------------------------------------------------------

/// Convert markdown to Slack mrkdwn format.
fn markdown_to_slack(input: &str) -> String {
    let mut lines: Vec<String> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // Headers → *bold text*
        if let Some(rest) = trimmed.strip_prefix("### ") {
            lines.push(format!("*{}*", rest.trim()));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            lines.push(format!("*{}*", rest.trim()));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            lines.push(format!("*{}*", rest.trim()));
            continue;
        }

        // Bullet markers: - or * at start → •
        let converted = if let Some(rest) = trimmed.strip_prefix("- ") {
            format!("• {rest}")
        } else if let Some(rest) = trimmed.strip_prefix("* ") {
            format!("• {rest}")
        } else {
            line.to_string()
        };

        lines.push(converted);
    }

    let mut result = lines.join("\n");

    // Inline links: [text](url) → <url|text>
    result = convert_links(&result);

    // Bold: **text** → *text*
    result = convert_bold(&result);

    result
}

/// Convert markdown links [text](url) to Slack format <url|text>.
fn convert_links(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '[' {
            // Try to parse [text](url)
            if let Some((text, url, end)) = parse_md_link(&chars, i) {
                out.push('<');
                out.push_str(&url);
                out.push('|');
                out.push_str(&text);
                out.push('>');
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

/// Try to parse a markdown link starting at position `start` (which should be '[').
/// Returns (text, url, end_position) if successful.
fn parse_md_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    // Find closing ]
    let mut i = start + 1;
    let mut text = String::new();
    while i < chars.len() && chars[i] != ']' {
        text.push(chars[i]);
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    // chars[i] == ']', next must be '('
    i += 1;
    if i >= chars.len() || chars[i] != '(' {
        return None;
    }
    i += 1;
    let mut url = String::new();
    while i < chars.len() && chars[i] != ')' {
        url.push(chars[i]);
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    // chars[i] == ')'
    Some((text, url, i + 1))
}

/// Convert markdown bold **text** to Slack bold *text*.
fn convert_bold(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            // Find the closing **
            if let Some(end) = find_closing_double_star(&chars, i + 2) {
                out.push('*');
                for &c in &chars[i + 2..end] {
                    out.push(c);
                }
                out.push('*');
                i = end + 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

fn find_closing_double_star(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '*' {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Webhook (mrkdwn) tests ---

    #[test]
    fn test_headers_to_bold() {
        assert_eq!(markdown_to_slack("# Big Header"), "*Big Header*");
        assert_eq!(markdown_to_slack("## Section"), "*Section*");
        assert_eq!(markdown_to_slack("### Sub"), "*Sub*");
    }

    #[test]
    fn test_bullets() {
        assert_eq!(markdown_to_slack("- item one"), "• item one");
        assert_eq!(markdown_to_slack("* item two"), "• item two");
    }

    #[test]
    fn test_bold() {
        assert_eq!(markdown_to_slack("this is **bold** text"), "this is *bold* text");
    }

    #[test]
    fn test_links() {
        assert_eq!(
            markdown_to_slack("check [this](https://example.com) out"),
            "check <https://example.com|this> out"
        );
    }

    #[test]
    fn test_combined() {
        let md = "# Daily Brief\n\n- **BTC** up 5%\n- Check [CoinDesk](https://coindesk.com)\n\nThis is a paragraph.";
        let expected = "*Daily Brief*\n\n• *BTC* up 5%\n• Check <https://coindesk.com|CoinDesk>\n\nThis is a paragraph.";
        assert_eq!(markdown_to_slack(md), expected);
    }

    #[test]
    fn test_passthrough() {
        assert_eq!(markdown_to_slack("plain text"), "plain text");
    }

    // --- Block Kit tests ---

    #[test]
    fn test_markdown_to_blocks_header() {
        let blocks = markdown_to_blocks("# My Header\nSome text");
        assert!(blocks.len() >= 2);
        match &blocks[0] {
            Block::Header { text } => {
                assert_eq!(text.kind, "plain_text");
                // May have emoji prefix
                assert!(text.text.contains("My Header"));
            }
            _ => panic!("expected Header block"),
        }
        match &blocks[1] {
            Block::Section { text } => {
                assert_eq!(text.kind, "mrkdwn");
                assert!(text.text.contains("Some text"));
            }
            _ => panic!("expected Section block"),
        }
    }

    #[test]
    fn test_markdown_to_blocks_divider() {
        let blocks = markdown_to_blocks("Above\n---\nBelow");
        assert!(blocks.len() >= 3);
        assert!(matches!(blocks[1], Block::Divider));
    }

    #[test]
    fn test_markdown_to_blocks_bullets_converted() {
        let blocks = markdown_to_blocks("- item one\n- item two");
        match &blocks[0] {
            Block::RichText { elements } => {
                match &elements[0] {
                    RichTextElement::List { style: _, elements: items } => {
                        assert_eq!(items.len(), 2);
                    }
                    _ => panic!("expected RichTextList"),
                }
            }
            _ => panic!("expected RichText block, got {:?}", blocks[0]),
        }
    }

    #[test]
    fn test_markdown_to_blocks_links_and_bold() {
        let blocks = markdown_to_blocks("Check **this** [link](https://example.com)");
        match &blocks[0] {
            Block::Section { text } => {
                assert!(text.text.contains("*this*"));
                assert!(text.text.contains("<https://example.com|link>"));
            }
            _ => panic!("expected Section"),
        }
    }

    #[test]
    fn test_markdown_to_blocks_long_header_truncated() {
        let long_header = format!("# {}", "A".repeat(200));
        let blocks = markdown_to_blocks(&long_header);
        match &blocks[0] {
            Block::Header { text } => {
                assert!(text.text.len() <= MAX_HEADER_LEN);
            }
            _ => panic!("expected Header"),
        }
    }

    #[test]
    fn test_block_kit_serialization() {
        let block = Block::Header {
            text: TextObject {
                kind: "plain_text",
                text: "Hello".to_string(),
            },
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "header");
        assert_eq!(json["text"]["type"], "plain_text");
        assert_eq!(json["text"]["text"], "Hello");
    }

    // --- New tests ---

    #[test]
    fn test_h2_becomes_bold_section() {
        let blocks = markdown_to_blocks("## My Section");
        match &blocks[0] {
            Block::Section { text } => {
                assert_eq!(text.kind, "mrkdwn");
                assert_eq!(text.text, "*My Section*");
            }
            _ => panic!("expected Section block, got {:?}", blocks[0]),
        }

        let blocks = markdown_to_blocks("### Sub Section");
        match &blocks[0] {
            Block::Section { text } => {
                assert_eq!(text.text, "*Sub Section*");
            }
            _ => panic!("expected Section block"),
        }
    }

    #[test]
    fn test_bullets_become_rich_text_list() {
        let blocks = markdown_to_blocks("- alpha\n- beta\n- gamma");
        match &blocks[0] {
            Block::RichText { elements } => {
                assert_eq!(elements.len(), 1);
                match &elements[0] {
                    RichTextElement::List { style: _, elements: items } => {
                        assert_eq!(items.len(), 3);
                        // First item should contain "alpha"
                        let text = extract_inline_text(&items[0].elements);
                        assert_eq!(text, "alpha");
                    }
                    _ => panic!("expected list"),
                }
            }
            _ => panic!("expected RichText block"),
        }
    }

    #[test]
    fn test_context_block_serialization() {
        let block = Block::Context {
            elements: vec![ContextElement::Mrkdwn {
                text: "5 PRs merged".to_string(),
            }],
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "context");
        assert_eq!(json["elements"][0]["type"], "mrkdwn");
        assert_eq!(json["elements"][0]["text"], "5 PRs merged");
    }

    #[test]
    fn test_rich_text_serialization() {
        let block = Block::RichText {
            elements: vec![RichTextElement::List {
                style: ListStyle::Bullet,
                elements: vec![RichTextListItem {
                    elements: vec![RichTextInline::Text {
                        text: "hello".to_string(),
                        style: Some(RichTextStyle { bold: true, ..Default::default() }),
                    }],
                }],
            }],
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "rich_text");
        assert_eq!(json["elements"][0]["type"], "rich_text_list");
        assert_eq!(json["elements"][0]["style"], "bullet");
        assert_eq!(json["elements"][0]["elements"][0]["type"], "rich_text_section");
        assert_eq!(json["elements"][0]["elements"][0]["elements"][0]["type"], "text");
        assert_eq!(json["elements"][0]["elements"][0]["elements"][0]["text"], "hello");
        assert_eq!(json["elements"][0]["elements"][0]["elements"][0]["style"]["bold"], true);
    }

    #[test]
    fn test_emoji_prefix_on_headers() {
        let blocks = markdown_to_blocks("# Dev Changelog, Week of Jan 6");
        match &blocks[0] {
            Block::Header { text } => {
                assert!(text.text.starts_with(":memo:"), "expected emoji prefix, got: {}", text.text);
            }
            _ => panic!("expected Header"),
        }

        // No double-prefix
        let blocks = markdown_to_blocks("# :ship: Already has emoji");
        match &blocks[0] {
            Block::Header { text } => {
                assert!(!text.text.starts_with(":ship: :"), "should not double-prefix: {}", text.text);
            }
            _ => panic!("expected Header"),
        }
    }

    #[test]
    fn test_metadata_becomes_context() {
        let blocks = markdown_to_blocks("5 PRs merged across 3 repos");
        match &blocks[0] {
            Block::Context { elements } => {
                match &elements[0] {
                    ContextElement::Mrkdwn { text } => {
                        assert!(text.contains("5 PRs merged"));
                    }
                }
            }
            _ => panic!("expected Context block, got {:?}", blocks[0]),
        }
    }

    #[test]
    fn test_parse_inline_bold() {
        let elems = parse_inline_elements("hello **world** end");
        assert_eq!(elems.len(), 3);
        match &elems[0] {
            RichTextInline::Text { text, style } => {
                assert_eq!(text, "hello ");
                assert!(style.is_none());
            }
            _ => panic!("expected Text"),
        }
        match &elems[1] {
            RichTextInline::Text { text, style } => {
                assert_eq!(text, "world");
                assert!(style.as_ref().unwrap().bold);
            }
            _ => panic!("expected bold Text"),
        }
    }

    #[test]
    fn test_parse_inline_link() {
        let elems = parse_inline_elements("see [docs](https://example.com) here");
        assert_eq!(elems.len(), 3);
        match &elems[1] {
            RichTextInline::Link { url, text, .. } => {
                assert_eq!(url, "https://example.com");
                assert_eq!(text.as_deref(), Some("docs"));
            }
            _ => panic!("expected Link"),
        }
    }

    #[test]
    fn test_parse_inline_emoji() {
        let elems = parse_inline_elements("hello :wave: world");
        assert_eq!(elems.len(), 3);
        match &elems[1] {
            RichTextInline::Emoji { name } => {
                assert_eq!(name, "wave");
            }
            _ => panic!("expected Emoji"),
        }
    }

    #[test]
    fn test_parse_inline_code() {
        let elems = parse_inline_elements("run `cargo test` now");
        assert_eq!(elems.len(), 3);
        match &elems[1] {
            RichTextInline::Text { text, style } => {
                assert_eq!(text, "cargo test");
                assert!(style.as_ref().unwrap().code);
            }
            _ => panic!("expected code Text"),
        }
    }

    #[test]
    fn test_mixed_bullets_and_paragraphs() {
        let blocks = markdown_to_blocks("Some intro\n\n- bullet one\n- bullet two\n\nAnother paragraph");
        // Should be: Section, RichText, Section
        assert!(blocks.len() >= 3, "got {} blocks: {:?}", blocks.len(), blocks);
        assert!(matches!(&blocks[0], Block::Section { .. }));
        assert!(matches!(&blocks[1], Block::RichText { .. }));
        assert!(matches!(&blocks[2], Block::Section { .. }));
    }

    #[test]
    fn test_divider_serialization() {
        let block = Block::Divider;
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "divider");
        // Should only have "type" key
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn test_section_serialization() {
        let block = Block::Section {
            text: TextObject {
                kind: "mrkdwn",
                text: "*bold section*".to_string(),
            },
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "section");
        assert_eq!(json["text"]["type"], "mrkdwn");
        assert_eq!(json["text"]["text"], "*bold section*");
    }
}

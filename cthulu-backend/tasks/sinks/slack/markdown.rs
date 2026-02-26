use super::blocks::*;

// ---------------------------------------------------------------------------
// Markdown → Block Kit blocks
// ---------------------------------------------------------------------------

/// Convert markdown text into Slack Block Kit blocks.
pub fn markdown_to_blocks(text: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut paragraph_lines: Vec<String> = Vec::new();
    let mut bullet_items: Vec<Vec<RichTextInline>> = Vec::new();
    let mut stats_lines: Option<Vec<String>> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        // [stats] / [/stats] block → SectionFields
        if trimmed.eq_ignore_ascii_case("[stats]") {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            flush_bullets(&mut blocks, &mut bullet_items);
            stats_lines = Some(Vec::new());
            continue;
        }
        if trimmed.eq_ignore_ascii_case("[/stats]") {
            if let Some(lines) = stats_lines.take() {
                flush_stats_block(&mut blocks, &lines);
            }
            continue;
        }
        if let Some(ref mut lines) = stats_lines {
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
            continue;
        }

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

/// Flush stats lines into a SectionFields block (2-column grid).
///
/// Each line becomes one field cell. Lines containing `|` are split into
/// multiple cells. All cells are mrkdwn so emoji shortcodes render.
fn flush_stats_block(blocks: &mut Vec<Block>, lines: &[String]) {
    if lines.is_empty() {
        return;
    }

    let mut fields: Vec<TextObject> = Vec::new();
    for line in lines {
        if line.contains('|') {
            for cell in line.split('|') {
                let cell = cell.trim();
                if !cell.is_empty() {
                    fields.push(TextObject { kind: "mrkdwn", text: cell.to_string() });
                }
            }
        } else {
            fields.push(TextObject { kind: "mrkdwn", text: line.clone() });
        }
    }

    // Slack allows max 10 fields per section block
    for chunk in fields.chunks(10) {
        blocks.push(Block::SectionFields {
            fields: chunk.to_vec(),
        });
    }
}

// ---------------------------------------------------------------------------
// Inline element parser for rich text
// ---------------------------------------------------------------------------

/// Parse inline markdown into rich text elements.
/// Handles: **bold**, `code`, [text](url), :emoji:, and plain text.
pub(crate) fn parse_inline_elements(text: &str) -> Vec<RichTextInline> {
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
pub fn markdown_to_slack(input: &str) -> String {
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
pub(crate) fn convert_links(input: &str) -> String {
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
pub(crate) fn convert_bold(input: &str) -> String {
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

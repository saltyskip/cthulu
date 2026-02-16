use anyhow::{Context, Result};
use serde_json::json;

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
}

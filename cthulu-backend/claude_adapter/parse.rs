// NOTE: This module requires `regex` as a dependency in Cargo.toml.
// Add: regex = "1"

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::LazyLock;

/// Token usage summary from a Claude CLI run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageSummary {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
}

/// Aggregated result from parsing all stream-json lines of a Claude CLI run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamJsonResult {
    pub session_id: Option<String>,
    pub model: String,
    pub cost_usd: Option<f64>,
    pub usage: Option<UsageSummary>,
    pub summary: String,
    pub result_json: Option<Value>,
}

impl Default for StreamJsonResult {
    fn default() -> Self {
        Self {
            session_id: None,
            model: String::new(),
            cost_usd: None,
            usage: None,
            summary: String::new(),
            result_json: None,
        }
    }
}

/// Whether the CLI output indicates a login/auth problem, and an optional login URL.
#[derive(Debug, Clone)]
pub struct LoginDetection {
    pub requires_login: bool,
    pub login_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Compiled regexes (initialized once)
// ---------------------------------------------------------------------------

static LOGIN_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)(?:not\s+logged\s+in|please\s+log\s+in|please\s+run\s+`?claude\s+login`?|login\s+required|requires\s+login|unauthorized|authentication\s+required)"
    )
    .expect("login regex must compile")
});

static LOGIN_URL_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"https?://[^\s\])<>]+(?:claude|anthropic|auth)[^\s\])<>]*")
        .expect("login URL regex must compile")
});

static MAX_TURNS_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)max(?:imum)?\s+turns?").expect("max turns regex must compile")
});

static UNKNOWN_SESSION_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)(?:no\s+conversation\s+found\s+with\s+session\s+id|unknown\s+session|session\s+\S+\s+not\s+found)"
    )
    .expect("unknown session regex must compile")
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a string field from a `serde_json::Value` map by key.
/// Returns an empty string if the key is absent or not a string.
fn as_str(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Collect all text content blocks from an assistant message into a single string.
fn extract_text_blocks(event: &Value) -> String {
    let content = event
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array);

    let Some(blocks) = content else {
        return String::new();
    };

    let mut parts: Vec<&str> = Vec::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                parts.push(text);
            }
        }
    }
    parts.join("\n")
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse all stdout from a Claude CLI run in `--output-format stream-json` format.
///
/// Each line of `stdout` is expected to be a self-contained JSON object.  The
/// function walks through every line and extracts:
///
/// * **system/init** events → `session_id`, `model`
/// * **assistant** events → text content blocks (appended to `summary`)
/// * **result** events → `usage`, `cost_usd`, `session_id`, final `result_json`
///
/// Lines that fail to parse as JSON are silently skipped.
pub fn parse_stream_json(stdout: &str) -> StreamJsonResult {
    let mut result = StreamJsonResult::default();
    let mut text_parts: Vec<String> = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let event: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(line = trimmed, "skipping unparseable stream-json line");
                continue;
            }
        };

        let event_type = as_str(&event, "type");
        match event_type.as_str() {
            "system" => {
                let subtype = as_str(&event, "subtype");
                if subtype == "init" {
                    if let Some(sid) = event.get("session_id").and_then(Value::as_str) {
                        result.session_id = Some(sid.to_string());
                    }
                    let model = as_str(&event, "model");
                    if !model.is_empty() {
                        result.model = model;
                    }
                }
            }
            "assistant" => {
                let text = extract_text_blocks(&event);
                if !text.is_empty() {
                    text_parts.push(text);
                }
            }
            "result" => {
                // Result event is the authoritative final record.
                result.result_json = Some(event.clone());

                // Session ID (may also appear in result)
                if let Some(sid) = event.get("session_id").and_then(Value::as_str) {
                    result.session_id = Some(sid.to_string());
                }

                // Cost
                if let Some(cost) = event
                    .get("cost_usd")
                    .or_else(|| event.get("total_cost_usd"))
                    .and_then(Value::as_f64)
                {
                    result.cost_usd = Some(cost);
                }

                // Usage
                if let Some(usage_val) = event.get("usage") {
                    let input_tokens = usage_val
                        .get("input_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let cached = usage_val
                        .get("cache_read_input_tokens")
                        .or_else(|| usage_val.get("cached_input_tokens"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let output_tokens = usage_val
                        .get("output_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    result.usage = Some(UsageSummary {
                        input_tokens,
                        cached_input_tokens: cached,
                        output_tokens,
                    });
                }

                // Result text (used as summary fallback)
                if let Some(text) = event.get("result").and_then(Value::as_str) {
                    text_parts.push(text.to_string());
                }
            }
            _ => {
                // Ignore unknown event types (e.g. "tool_use", "progress", etc.)
            }
        }
    }

    result.summary = text_parts.join("\n");
    result
}

/// Check whether Claude's output indicates that login/authentication is required.
///
/// Inspects both `stdout` and `stderr` for common authentication-failure
/// patterns emitted by the Claude CLI.  If a URL containing "claude",
/// "anthropic", or "auth" is found, it is returned as `login_url`.
pub fn detect_login_required(stdout: &str, stderr: &str) -> LoginDetection {
    let combined = format!("{stdout}\n{stderr}");

    let requires_login = LOGIN_RE.is_match(&combined);

    let login_url = if requires_login {
        LOGIN_URL_RE.find(&combined).map(|m| m.as_str().to_string())
    } else {
        None
    };

    LoginDetection {
        requires_login,
        login_url,
    }
}

/// Check if a Claude result event indicates that the maximum number of turns
/// was exhausted.
///
/// Looks for:
/// * `subtype == "error_max_turns"`
/// * `stop_reason == "max_turns"`
/// * Result text matching the pattern `max turns` / `maximum turns`
pub fn is_max_turns_result(parsed: &Value) -> bool {
    if as_str(parsed, "subtype") == "error_max_turns" {
        return true;
    }
    if as_str(parsed, "stop_reason") == "max_turns" {
        return true;
    }

    // Check the result text
    if let Some(text) = parsed.get("result").and_then(Value::as_str) {
        if MAX_TURNS_RE.is_match(text) {
            return true;
        }
    }

    false
}

/// Check if a Claude result event indicates an unknown or expired session.
///
/// This is useful for automatic retry logic: if the session is gone, the
/// caller can drop the stale session ID and start a fresh conversation.
pub fn is_unknown_session_error(parsed: &Value) -> bool {
    // Check the result text
    if let Some(text) = parsed.get("result").and_then(Value::as_str) {
        if UNKNOWN_SESSION_RE.is_match(text) {
            return true;
        }
    }

    // Check the errors array
    if let Some(errors) = parsed.get("errors").and_then(Value::as_array) {
        for error in errors {
            let msg = match error {
                Value::String(s) => s.clone(),
                _ => as_str(error, "message"),
            };
            if UNKNOWN_SESSION_RE.is_match(&msg) {
                return true;
            }
        }
    }

    // Also check a top-level "error" string field
    if let Some(err) = parsed.get("error").and_then(Value::as_str) {
        if UNKNOWN_SESSION_RE.is_match(err) {
            return true;
        }
    }

    false
}

/// Build a human-readable failure description from a Claude result event.
///
/// Returns `None` if the event does not appear to represent a failure (i.e. no
/// error subtype and no error details found).
pub fn describe_failure(parsed: &Value) -> Option<String> {
    let subtype = as_str(parsed, "subtype");
    let result_text = parsed
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // Collect error messages from the errors array
    let error_messages: Vec<String> = parsed
        .get("errors")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|e| match e {
                    Value::String(s) => s.clone(),
                    _ => as_str(e, "message"),
                })
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Also check top-level "error" field
    let top_error = parsed
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // Determine if this looks like a failure at all
    let has_error_subtype = !subtype.is_empty() && subtype.starts_with("error");
    let has_error_detail =
        !result_text.is_empty() || !error_messages.is_empty() || !top_error.is_empty();

    if !has_error_subtype && !has_error_detail {
        return None;
    }

    // Build the description
    let mut parts: Vec<String> = Vec::new();

    if !subtype.is_empty() {
        parts.push(format!("subtype={subtype}"));
    }

    // Prefer the errors array, then top-level error, then result text
    if !error_messages.is_empty() {
        parts.push(error_messages.join("; "));
    } else if !top_error.is_empty() {
        parts.push(top_error);
    } else if !result_text.is_empty() {
        // Truncate long result text for readability
        let truncated = if result_text.len() > 200 {
            format!("{}…", &result_text[..200])
        } else {
            result_text
        };
        parts.push(truncated);
    }

    Some(format!("Claude run failed: {}", parts.join(": ")))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stream_json_basic() {
        let stdout = r#"{"type":"system","subtype":"init","session_id":"sess-123","model":"claude-sonnet-4-20250514"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello there!"}]}}
{"type":"result","session_id":"sess-123","cost_usd":0.003,"usage":{"input_tokens":100,"output_tokens":50},"result":"Hello there!"}"#;

        let result = parse_stream_json(stdout);
        assert_eq!(result.session_id.as_deref(), Some("sess-123"));
        assert_eq!(result.model, "claude-sonnet-4-20250514");
        assert_eq!(result.cost_usd, Some(0.003));
        assert!(result.usage.is_some());
        let usage = result.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert!(result.summary.contains("Hello there!"));
    }

    #[test]
    fn test_parse_stream_json_skips_bad_lines() {
        let stdout = "not json at all\n{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-sonnet-4-20250514\"}\n{malformed\n";
        let result = parse_stream_json(stdout);
        assert_eq!(result.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_parse_stream_json_empty() {
        let result = parse_stream_json("");
        assert!(result.session_id.is_none());
        assert!(result.model.is_empty());
        assert!(result.summary.is_empty());
    }

    #[test]
    fn test_detect_login_required_positive() {
        let det = detect_login_required("", "Error: not logged in. Please run `claude login`.");
        assert!(det.requires_login);
    }

    #[test]
    fn test_detect_login_required_with_url() {
        let stderr = "Please log in at https://claude.anthropic.com/auth/login to continue.";
        let det = detect_login_required("", stderr);
        assert!(det.requires_login);
        assert!(det.login_url.is_some());
        assert!(det.login_url.unwrap().contains("anthropic"));
    }

    #[test]
    fn test_detect_login_required_negative() {
        let det = detect_login_required("Hello world", "Some warning");
        assert!(!det.requires_login);
        assert!(det.login_url.is_none());
    }

    #[test]
    fn test_is_max_turns_result_subtype() {
        let v: Value = serde_json::json!({"subtype": "error_max_turns"});
        assert!(is_max_turns_result(&v));
    }

    #[test]
    fn test_is_max_turns_result_stop_reason() {
        let v: Value = serde_json::json!({"stop_reason": "max_turns"});
        assert!(is_max_turns_result(&v));
    }

    #[test]
    fn test_is_max_turns_result_text() {
        let v: Value = serde_json::json!({"result": "Stopped because maximum turns reached."});
        assert!(is_max_turns_result(&v));
    }

    #[test]
    fn test_is_max_turns_result_negative() {
        let v: Value = serde_json::json!({"result": "All done!"});
        assert!(!is_max_turns_result(&v));
    }

    #[test]
    fn test_is_unknown_session_error_result() {
        let v: Value =
            serde_json::json!({"result": "Error: no conversation found with session id abc-123"});
        assert!(is_unknown_session_error(&v));
    }

    #[test]
    fn test_is_unknown_session_error_errors_array() {
        let v: Value = serde_json::json!({"errors": ["unknown session"]});
        assert!(is_unknown_session_error(&v));
    }

    #[test]
    fn test_is_unknown_session_error_negative() {
        let v: Value = serde_json::json!({"result": "Success!"});
        assert!(!is_unknown_session_error(&v));
    }

    #[test]
    fn test_describe_failure_with_subtype_and_errors() {
        let v: Value = serde_json::json!({
            "subtype": "error_max_turns",
            "errors": ["max turns exceeded"]
        });
        let desc = describe_failure(&v).unwrap();
        assert!(desc.contains("error_max_turns"));
        assert!(desc.contains("max turns exceeded"));
    }

    #[test]
    fn test_describe_failure_returns_none_for_success() {
        let v: Value = serde_json::json!({"subtype": "success", "result": ""});
        assert!(describe_failure(&v).is_none());
    }

    #[test]
    fn test_describe_failure_with_error_field() {
        let v: Value = serde_json::json!({"error": "something went wrong"});
        let desc = describe_failure(&v).unwrap();
        assert!(desc.contains("something went wrong"));
    }

    #[test]
    fn test_parse_stream_json_cached_input_tokens() {
        let stdout = r#"{"type":"result","usage":{"input_tokens":200,"cache_read_input_tokens":150,"output_tokens":80}}"#;
        let result = parse_stream_json(stdout);
        let usage = result.usage.unwrap();
        assert_eq!(usage.cached_input_tokens, 150);
    }
}

//! VM Executor — runs the Claude CLI inside a Firecracker VM via ttyd WebSocket.
//!
//! Instead of spawning a local `claude` subprocess, this executor:
//! 1. Connects to the VM's ttyd web terminal via WebSocket
//! 2. Writes the prompt to a temp file inside the VM
//! 3. Runs `claude --print --output-format stream-json ... - < /tmp/cthulu_prompt.txt`
//! 4. Captures the stream-json output
//! 5. Parses for the `result` event to extract text, cost, and turns
//!
//! The VM must already be provisioned and have Claude CLI installed with
//! OAuth credentials injected (done during flow enable).

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;

use super::{ExecutionResult, Executor};
use crate::sandbox::ttyd::TtydSession;

/// Timeout for a single Claude execution inside a VM (15 minutes).
const VM_EXEC_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub struct VmExecutor {
    web_terminal_url: String,
    permissions: Vec<String>,
    append_system_prompt: Option<String>,
}

impl VmExecutor {
    pub fn new(
        web_terminal_url: String,
        permissions: Vec<String>,
        append_system_prompt: Option<String>,
    ) -> Self {
        Self {
            web_terminal_url,
            permissions,
            append_system_prompt,
        }
    }

    /// Default tool permissions used when no explicit permissions are given.
    /// VMs run as root, so `--dangerously-skip-permissions` is rejected by
    /// Claude CLI. Instead we grant a broad set of tools explicitly.
    const DEFAULT_VM_TOOLS: &'static [&'static str] = &[
        "Bash(*)", "Read(*)", "Write(*)", "Edit(*)", "Glob(*)", "Grep(*)",
        "WebFetch(*)", "WebSearch(*)", "TodoWrite(*)",
    ];

    /// Build the claude CLI command string to run inside the VM.
    fn build_claude_command(&self) -> String {
        let mut parts = vec![
            "claude".to_string(),
            "--print".to_string(),
            "--verbose".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];

        if let Some(prompt) = &self.append_system_prompt {
            // Shell-escape the system prompt
            let escaped = prompt.replace('\'', "'\\''");
            parts.push("--append-system-prompt".to_string());
            parts.push(format!("'{escaped}'"));
        }

        // VMs run as root — Claude CLI rejects --dangerously-skip-permissions
        // for root. Use explicit tool allowlist instead.
        let tools: Vec<String> = if self.permissions.is_empty() {
            Self::DEFAULT_VM_TOOLS.iter().map(|s| s.to_string()).collect()
        } else {
            self.permissions.clone()
        };
        parts.push("--allowedTools".to_string());
        parts.push(tools.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(" "));

        parts.push("-".to_string()); // read from stdin
        parts.join(" ")
    }
}

#[async_trait]
impl Executor for VmExecutor {
    async fn execute(&self, prompt: &str, _working_dir: &Path) -> Result<ExecutionResult> {
        tracing::info!(
            url = %self.web_terminal_url,
            "connecting to VM ttyd for execution"
        );

        let mut session = TtydSession::connect(&self.web_terminal_url)
            .await
            .context("failed to connect to VM ttyd")?;

        // Build the prompt file, source credentials, and run claude — all in one command.
        // This avoids stale-marker contamination between sequential exec() calls.
        let prompt_b64 = base64_encode(prompt.as_bytes());
        let claude_cmd = self.build_claude_command();

        let combined_cmd = format!(
            "echo '{}' | base64 -d > /tmp/cthulu_prompt.txt && source ~/.bashrc 2>/dev/null && {} < /tmp/cthulu_prompt.txt",
            prompt_b64, claude_cmd
        );

        tracing::info!(cmd = %claude_cmd, "executing Claude CLI in VM");

        let raw_output = session
            .exec(&combined_cmd, VM_EXEC_TIMEOUT)
            .await
            .context("Claude CLI execution in VM failed")?;

        // Step 3: Parse the stream-json output for result event
        let result = parse_stream_json_output(&raw_output);

        tracing::info!(
            cost = format_args!("${:.4}", result.cost_usd),
            turns = result.num_turns,
            output_len = result.text.len(),
            "VM executor completed"
        );

        // Clean up
        let _ = session
            .exec("rm -f /tmp/cthulu_prompt.txt", Duration::from_secs(5))
            .await;
        session.close().await;

        Ok(result)
    }
}

/// Parse stream-json output from Claude CLI.
///
/// Each line is a JSON event. We look for the `"result"` event which contains:
/// - `total_cost_usd`: f64
/// - `num_turns`: u64
/// - `result`: String (the final text output)
///
/// If the stream-json parsing fails (e.g., terminal noise), we fall back
/// to treating the entire output as the result text.
fn parse_stream_json_output(raw: &str) -> ExecutionResult {
    let mut result_text: Option<String> = None;
    let mut total_cost: f64 = 0.0;
    let mut total_turns: u64 = 0;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let event_type = event
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if event_type == "result" {
                total_cost = event
                    .get("total_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                total_turns = event
                    .get("num_turns")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                result_text = event
                    .get("result")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }
    }

    // If we found a result event, use it. Otherwise, fall back to the raw output.
    if let Some(text) = result_text {
        ExecutionResult {
            text,
            cost_usd: total_cost,
            num_turns: total_turns,
        }
    } else {
        tracing::warn!("No stream-json result event found in VM output, using raw output");
        ExecutionResult {
            text: raw.to_string(),
            cost_usd: 0.0,
            num_turns: 0,
        }
    }
}

/// Simple base64 encoding without external dependency.
/// Uses the standard alphabet (A-Z, a-z, 0-9, +, /) with = padding.
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((n >> 18) & 63) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_claude_command_no_permissions() {
        let exec = VmExecutor::new(
            "http://localhost:7700".into(),
            vec![],
            None,
        );
        let cmd = exec.build_claude_command();
        // VMs run as root so we use explicit tools, not --dangerously-skip-permissions
        assert!(cmd.contains("--allowedTools"));
        assert!(cmd.contains("Bash(*)"));
        assert!(cmd.contains("--output-format stream-json"));
        assert!(cmd.ends_with(" -"));
    }

    #[test]
    fn test_build_claude_command_with_permissions() {
        let exec = VmExecutor::new(
            "http://localhost:7700".into(),
            vec!["Bash".into(), "Read".into()],
            None,
        );
        let cmd = exec.build_claude_command();
        assert!(cmd.contains("--allowedTools"));
        assert!(cmd.contains("Bash"));
        assert!(cmd.contains("Read"));
    }

    #[test]
    fn test_build_claude_command_with_system_prompt() {
        let exec = VmExecutor::new(
            "http://localhost:7700".into(),
            vec![],
            Some("You are a helpful assistant.".into()),
        );
        let cmd = exec.build_claude_command();
        assert!(cmd.contains("--append-system-prompt"));
        assert!(cmd.contains("You are a helpful assistant."));
    }

    #[test]
    fn test_parse_stream_json_output() {
        let raw = r#"{"type":"system","subtype":"init","session_id":"abc"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello!"}]}}
{"type":"result","result":"Final answer","total_cost_usd":0.0123,"num_turns":3}
"#;
        let result = parse_stream_json_output(raw);
        assert_eq!(result.text, "Final answer");
        assert!((result.cost_usd - 0.0123).abs() < f64::EPSILON);
        assert_eq!(result.num_turns, 3);
    }

    #[test]
    fn test_parse_stream_json_no_result_event() {
        let raw = "Some terminal output\nwithout JSON events\n";
        let result = parse_stream_json_output(raw);
        assert_eq!(result.text, raw);
        assert_eq!(result.cost_usd, 0.0);
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        assert_eq!(base64_encode(b"Hi"), "SGk=");
        assert_eq!(base64_encode(b"Foo"), "Rm9v");
        assert_eq!(base64_encode(b""), "");
    }
}

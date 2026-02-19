use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use super::{ExecutionResult, Executor};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub struct ClaudeCodeExecutor {
    permissions: Vec<String>,
    append_system_prompt: Option<String>,
}

impl ClaudeCodeExecutor {
    pub fn new(permissions: Vec<String>, append_system_prompt: Option<String>) -> Self {
        Self { permissions, append_system_prompt }
    }

    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            "--print".to_string(),
            "--verbose".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];

        if let Some(prompt) = &self.append_system_prompt {
            args.push("--append-system-prompt".to_string());
            args.push(prompt.clone());
        }

        if self.permissions.is_empty() {
            args.push("--dangerously-skip-permissions".to_string());
        } else {
            args.push("--allowedTools".to_string());
            args.push(self.permissions.join(","));
        }

        args.push("-".to_string()); // read from stdin
        args
    }
}

#[async_trait]
impl Executor for ClaudeCodeExecutor {
    async fn execute(&self, prompt: &str, working_dir: &Path) -> Result<ExecutionResult> {
        let args = self.build_args();

        let mut child = Command::new("claude")
            .args(&args)
            .current_dir(working_dir)
            .env_remove("CLAUDECODE")
            .env("CLAUDECODE", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn claude process")?;

        // Write prompt to stdin
        {
            use tokio::io::AsyncWriteExt;
            let mut stdin = child.stdin.take().expect("stdin piped");
            if let Err(e) = stdin.write_all(prompt.as_bytes()).await {
                let _ = child.kill().await;
                return Err(e).context("failed to write prompt to stdin");
            }
        }

        // Stream stderr to tracing
        let stderr = child.stderr.take().expect("stderr piped");
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.is_empty() {
                    tracing::debug!(source = "claude-stderr", "{}", line);
                }
            }
        });

        // Stream stdout JSON events to tracing, capture result
        let stdout = child.stdout.take().expect("stdout piped");
        let stdout_handle = tokio::spawn(async move {
            let mut result_text: Option<String> = None;
            let mut total_cost: f64 = 0.0;
            let mut total_turns: u64 = 0;

            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.is_empty() {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
                    let event_type = event
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    match event_type {
                        "system" => {
                            tracing::debug!(source = "claude", "Session initialized");
                        }
                        "assistant" => {
                            if let Some(content) = event
                                .get("message")
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_array())
                            {
                                for block in content {
                                    let block_type = block
                                        .get("type")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    match block_type {
                                        "tool_use" => {
                                            let tool = block
                                                .get("name")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("?");
                                            tracing::debug!(
                                                source = "claude",
                                                tool,
                                                "Tool: {}",
                                                tool,
                                            );
                                        }
                                        "text" => {
                                            let text_len = block
                                                .get("text")
                                                .and_then(|v| v.as_str())
                                                .map(|t| t.len())
                                                .unwrap_or(0);
                                            tracing::debug!(
                                                source = "claude",
                                                len = text_len,
                                                "Text output ({} chars)",
                                                text_len,
                                            );
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        "result" => {
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
                            tracing::debug!(
                                source = "claude",
                                cost = format_args!("${:.4}", total_cost),
                                turns = total_turns,
                                "Claude finished",
                            );
                        }
                        _ => {}
                    }
                }
            }

            (result_text, total_cost, total_turns)
        });

        let status = match timeout(PROCESS_TIMEOUT, child.wait()).await {
            Ok(result) => result.context("failed to wait on claude")?,
            Err(_elapsed) => {
                tracing::error!(
                    "claude process timed out after {}s, killing",
                    PROCESS_TIMEOUT.as_secs()
                );
                let _ = child.kill().await;
                stderr_handle.abort();
                stdout_handle.abort();
                anyhow::bail!("claude process timed out after {}s", PROCESS_TIMEOUT.as_secs());
            }
        };
        let _ = stderr_handle.await;
        let (result_text, cost_usd, num_turns) = stdout_handle
            .await
            .unwrap_or((None, 0.0, 0));

        if !status.success() {
            anyhow::bail!("claude exited with {}", status);
        }

        Ok(ExecutionResult {
            text: result_text.unwrap_or_default(),
            cost_usd,
            num_turns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_with_permissions() {
        let executor = ClaudeCodeExecutor::new(vec![
            "Bash".to_string(),
            "Read".to_string(),
            "Grep".to_string(),
        ], None);
        let args = executor.build_args();
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--verbose".to_string()));
        assert!(args.contains(&"--allowedTools".to_string()));
        assert!(args.contains(&"Bash,Read,Grep".to_string()));
        assert!(!args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn test_build_args_no_permissions_uses_dangerous() {
        let executor = ClaudeCodeExecutor::new(vec![], None);
        let args = executor.build_args();
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(!args.contains(&"--allowedTools".to_string()));
    }

    #[test]
    fn test_build_args_single_permission() {
        let executor = ClaudeCodeExecutor::new(vec!["Read".to_string()], None);
        let args = executor.build_args();
        assert!(args.contains(&"--allowedTools".to_string()));
        assert!(args.contains(&"Read".to_string()));
    }

    #[test]
    fn test_build_args_always_reads_stdin() {
        let executor = ClaudeCodeExecutor::new(vec![], None);
        let args = executor.build_args();
        assert_eq!(args.last().unwrap(), "-");
    }

    #[test]
    fn test_build_args_output_format() {
        let executor = ClaudeCodeExecutor::new(vec![], None);
        let args = executor.build_args();
        let fmt_idx = args.iter().position(|a| a == "--output-format").unwrap();
        assert_eq!(args[fmt_idx + 1], "stream-json");
    }
}

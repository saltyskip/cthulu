use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::Executor;

pub struct ClaudeCodeExecutor {
    permissions: Vec<String>,
}

impl ClaudeCodeExecutor {
    pub fn new(permissions: Vec<String>) -> Self {
        Self { permissions }
    }

    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            "--print".to_string(),
            "--verbose".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];

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
    async fn execute(&self, prompt: &str, working_dir: &Path) -> Result<()> {
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
            stdin
                .write_all(prompt.as_bytes())
                .await
                .context("failed to write prompt to stdin")?;
        }

        // Stream stderr to tracing
        let stderr = child.stderr.take().expect("stderr piped");
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.is_empty() {
                    tracing::info!(source = "claude-stderr", "{}", line);
                }
            }
        });

        // Stream stdout JSON events to tracing
        let stdout = child.stdout.take().expect("stdout piped");
        let stdout_handle = tokio::spawn(async move {
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
                            tracing::info!(source = "claude", "Session initialized");
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
                                            let input = block
                                                .get("input")
                                                .map(|v| v.to_string())
                                                .unwrap_or_default();
                                            let input_short = if input.len() > 300 {
                                                format!("{}...", &input[..300])
                                            } else {
                                                input
                                            };
                                            tracing::info!(
                                                source = "claude",
                                                tool,
                                                "Tool: {} {}",
                                                tool,
                                                input_short
                                            );
                                        }
                                        "text" => {
                                            let text = block
                                                .get("text")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("");
                                            let text_short = if text.len() > 200 {
                                                format!("{}...", &text[..200])
                                            } else {
                                                text.to_string()
                                            };
                                            tracing::info!(
                                                source = "claude",
                                                "Text: {}",
                                                text_short
                                            );
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        "result" => {
                            let cost = event
                                .get("total_cost_usd")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);
                            let turns = event
                                .get("num_turns")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            tracing::info!(
                                source = "claude",
                                cost_usd = cost,
                                turns,
                                "Claude finished - {} turns, ${:.4}",
                                turns,
                                cost
                            );
                        }
                        _ => {}
                    }
                }
            }
        });

        let status = child.wait().await.context("failed to wait on claude")?;
        let _ = stderr_handle.await;
        let _ = stdout_handle.await;

        if !status.success() {
            anyhow::bail!("claude exited with {}", status);
        }

        Ok(())
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
        ]);
        let args = executor.build_args();
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--verbose".to_string()));
        assert!(args.contains(&"--allowedTools".to_string()));
        assert!(args.contains(&"Bash,Read,Grep".to_string()));
        assert!(!args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn test_build_args_no_permissions_uses_dangerous() {
        let executor = ClaudeCodeExecutor::new(vec![]);
        let args = executor.build_args();
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(!args.contains(&"--allowedTools".to_string()));
    }

    #[test]
    fn test_build_args_single_permission() {
        let executor = ClaudeCodeExecutor::new(vec!["Read".to_string()]);
        let args = executor.build_args();
        assert!(args.contains(&"--allowedTools".to_string()));
        assert!(args.contains(&"Read".to_string()));
    }

    #[test]
    fn test_build_args_always_reads_stdin() {
        let executor = ClaudeCodeExecutor::new(vec![]);
        let args = executor.build_args();
        assert_eq!(args.last().unwrap(), "-");
    }

    #[test]
    fn test_build_args_output_format() {
        let executor = ClaudeCodeExecutor::new(vec![]);
        let args = executor.build_args();
        let fmt_idx = args.iter().position(|a| a == "--output-format").unwrap();
        assert_eq!(args[fmt_idx + 1], "stream-json");
    }
}

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use super::app::SessionInfo;

#[derive(Debug)]
#[allow(dead_code)]
pub enum ClaudeEvent {
    System(String),
    Text(String),
    ToolUse { tool: String, input: String },
    ToolResult(String),
    Result { text: String, cost: f64, turns: u64 },
    Error(String),
    Done(i32),
}

pub struct ClaudeStream {
    rx: mpsc::UnboundedReceiver<ClaudeEvent>,
    child: Option<Child>,
}

impl ClaudeStream {
    /// Spawn an interactive Claude Code CLI session.
    pub fn spawn(prompt: &str, session: &SessionInfo) -> anyhow::Result<Self> {
        let mut args = vec![
            "--print".to_string(),
            "--verbose".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];

        if let Some(sys_prompt) = &session.append_system_prompt {
            args.push("--append-system-prompt".to_string());
            args.push(sys_prompt.clone());
        }

        if session.permissions.is_empty() {
            args.push("--dangerously-skip-permissions".to_string());
        } else {
            args.push("--allowedTools".to_string());
            args.push(session.permissions.join(","));
        }

        // Read prompt from stdin
        args.push("-".to_string());

        let mut child = Command::new("claude")
            .args(&args)
            .current_dir(&session.working_dir)
            .env_remove("CLAUDECODE")
            .env("CLAUDECODE", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Write prompt to stdin then close it
        let prompt_bytes = prompt.as_bytes().to_vec();
        let mut stdin = child.stdin.take().expect("stdin piped");

        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn stdin writer
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = stdin.write_all(&prompt_bytes).await {
                let _ = tx_clone.send(ClaudeEvent::Error(format!("stdin write failed: {e}")));
                return;
            }
            drop(stdin); // close stdin to signal EOF
        });

        // Spawn stderr reader
        let stderr = child.stderr.take().expect("stderr piped");
        let tx_stderr = tx.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.is_empty() {
                    // Don't flood the UI with stderr, just log errors
                    let _ = tx_stderr.send(ClaudeEvent::System(format!("stderr: {line}")));
                }
            }
        });

        // Spawn stdout reader (stream-json parser)
        let stdout = child.stdout.take().expect("stdout piped");
        let tx_stdout = tx.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                if line.is_empty() {
                    continue;
                }

                let events = match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(json) => parse_stream_json(&json),
                    Err(_) => {
                        // Not JSON, just raw text
                        vec![ClaudeEvent::Text(line)]
                    }
                };

                for ev in events {
                    if tx_stdout.send(ev).is_err() {
                        break; // receiver dropped
                    }
                }
            }
        });

        // The tx sender is dropped here; stdout/stderr readers hold their own clones.
        // When the process exits, stdout closes, the reader task ends, and the
        // stream naturally reports no more events.
        drop(tx);

        Ok(Self {
            rx,
            child: Some(child),
        })
    }

    /// Try to receive the next event without blocking.
    pub fn try_recv(&mut self) -> Option<ClaudeEvent> {
        self.rx.try_recv().ok()
    }

    /// Kill the Claude process.
    pub fn kill(mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
}

impl Drop for ClaudeStream {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
}

fn parse_stream_json(json: &serde_json::Value) -> Vec<ClaudeEvent> {
    let event_type = match json.get("type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return vec![],
    };

    match event_type {
        "system" => {
            vec![ClaudeEvent::System("Session initialized".to_string())]
        }
        "assistant" => {
            let content = match json
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                Some(c) => c,
                None => return vec![],
            };

            let mut events = Vec::new();
            for block in content {
                let block_type = match block.get("type").and_then(|v| v.as_str()) {
                    Some(t) => t,
                    None => continue,
                };
                match block_type {
                    "tool_use" => {
                        let tool = block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?")
                            .to_string();
                        let input = block
                            .get("input")
                            .map(|v| {
                                if v.is_string() {
                                    v.as_str().unwrap_or("").to_string()
                                } else {
                                    serde_json::to_string_pretty(v).unwrap_or_default()
                                }
                            })
                            .unwrap_or_default();
                        events.push(ClaudeEvent::ToolUse { tool, input });
                    }
                    "tool_result" => {
                        let content = block
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        events.push(ClaudeEvent::ToolResult(content));
                    }
                    "text" => {
                        let text = block
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !text.is_empty() {
                            events.push(ClaudeEvent::Text(text));
                        }
                    }
                    _ => {}
                }
            }

            events
        }
        "result" => {
            let cost = json
                .get("total_cost_usd")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let turns = json
                .get("num_turns")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let text = json
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            vec![ClaudeEvent::Result { text, cost, turns }]
        }
        _ => vec![],
    }
}

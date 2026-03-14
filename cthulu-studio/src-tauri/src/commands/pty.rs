use cthulu::api::AppState;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

/// Workflow schema documentation written to CLAUDE.md in each workflow directory.
/// Claude Code auto-reads this file from the working directory.
const WORKFLOW_CLAUDE_MD: &str = r##"# Cthulu Workflow Assistant

You are working inside a Cthulu workflow directory. Your job is to help create, edit, and debug workflow definitions.

## Workflow File

The workflow definition lives in `workflow.yaml` in this directory. Read it to understand the current state. Edit it directly to make changes.

## Workflow Schema

A workflow is a DAG pipeline with nodes and edges:

```yaml
name: my-workflow
description: What this workflow does
enabled: true
nodes:
  - id: "n1"
    node_type: trigger
    kind: cron
    label: "Every 4 hours"
    config:
      schedule: "0 */4 * * *"
    position: { x: 0, y: 0 }
  - id: "n2"
    node_type: source
    kind: rss
    label: "RSS Feed"
    config:
      url: "https://example.com/feed"
      limit: 20
    position: { x: 250, y: 0 }
  - id: "n3"
    node_type: executor
    kind: claude-code
    label: "Summarizer"
    config:
      prompt: "Summarize:\n\n{{content}}"
    position: { x: 500, y: 0 }
  - id: "n4"
    node_type: sink
    kind: slack
    label: "Post to Slack"
    config:
      channel: "#general"
    position: { x: 750, y: 0 }
edges:
  - id: "e1"
    source: "n1"
    target: "n2"
  - id: "e2"
    source: "n2"
    target: "n3"
  - id: "e3"
    source: "n3"
    target: "n4"
```

## Node Types

### Triggers (start the pipeline)
| Kind | Config Fields |
|------|--------------|
| `cron` | `schedule` (cron expression) |
| `github-pr` | `repo` (owner/name) |
| `webhook` | (no config) |
| `manual` | (no config) |

### Sources (fetch data)
| Kind | Config Fields |
|------|--------------|
| `rss` | `url`, `limit?`, `keywords?` |
| `web-scrape` | `url` |
| `web-scraper` | `url`, `items_selector`, `title_selector`, `url_selector` |
| `github-merged-prs` | `repos` (array), `since_days?` |
| `market-data` | (no config) |
| `google-sheets` | `spreadsheet_id`, `range`, `service_account_key_env` |

### Filters (optional, between source and executor)
| Kind | Config Fields |
|------|--------------|
| `keyword` | `keywords` (array), `mode?` (any/all), `field?` |

### Executors (AI processing)
| Kind | Config Fields |
|------|--------------|
| `claude-code` | `prompt` (REQUIRED), `permissions?`, `working_dir?` |

### Sinks (output destinations)
| Kind | Config Fields |
|------|--------------|
| `slack` | `webhook_url_env?`, `bot_token_env?`, `channel?` |
| `notion` | `token_env`, `database_id` |

## Edge Wiring

Pipeline order: trigger → source → [filter →] executor → sink

Each edge needs a unique `id`, a `source` node ID, and a `target` node ID.

## Prompt Variables

Available in executor prompt templates:
`{{content}}`, `{{item_count}}`, `{{timestamp}}`, `{{market_data}}`, `{{diff}}`, `{{pr_number}}`, `{{pr_title}}`, `{{repo}}`

## Rules

- Always read `workflow.yaml` before making changes.
- Use meaningful node labels.
- Give each node a unique `id` (e.g. n1, n2, n3...).
- Give each edge a unique `id` (e.g. e1, e2, e3...).
- Position nodes left-to-right with ~250px spacing on the x-axis.
- Be efficient: short answers, batch tool calls when possible.
"##;

/// Write CLAUDE.md into a workflow directory if it doesn't already exist.
/// Preserves user edits — only creates the file on first use.
fn ensure_workflow_claude_md(workflow_dir: &Path) {
    let claude_md_path = workflow_dir.join("CLAUDE.md");
    if !claude_md_path.exists() {
        let _ = std::fs::create_dir_all(workflow_dir);
        let _ = std::fs::write(&claude_md_path, WORKFLOW_CLAUDE_MD);
    }
}

/// A live PTY session for an agent.
pub struct PtySession {
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    pub child: Arc<Mutex<Box<dyn portable_pty::Child + Send>>>,
    pub reader_handle: tokio::task::JoinHandle<()>,
}

/// Tauri-managed state for PTY sessions.
/// Separate from AppState because portable-pty types are not Clone.
pub struct PtyState {
    pub sessions: Arc<Mutex<HashMap<String, PtySession>>>,
}

impl PtyState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Drop for PtyState {
    fn drop(&mut self) {
        // Best-effort kill all PTY child processes on app exit
        if let Ok(mut sessions) = self.sessions.try_lock() {
            for (sid, session) in sessions.drain() {
                if let Ok(mut child) = session.child.try_lock() {
                    let _ = child.kill();
                }
                session.reader_handle.abort();
                eprintln!("Cleaned up PTY session: {}", sid);
            }
        }
    }
}

#[tauri::command]
pub async fn spawn_pty(
    state: tauri::State<'_, AppState>,
    pty_state: tauri::State<'_, PtyState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    app: tauri::AppHandle,
    agent_id: String,
    session_id: String,
    working_dir_override: Option<String>,
    workspace: Option<String>,
    workflow_name: Option<String>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;

    // Check if PTY already exists for this session (idempotent)
    {
        let sessions = pty_state.sessions.lock().await;
        if sessions.contains_key(&session_id) {
            return Ok(json!({ "session_id": session_id }));
        }
    }

    // Build the claude command based on whether we have an agent or a bare working dir
    let mut cmd = CommandBuilder::new("claude");
    let working_dir: String;

    // Resolve working directory: workspace+workflow → per-workflow dir, or explicit override
    let resolved_override = if let Some(ref ws) = workspace {
        // Per-workflow directory: ~/.cthulu/cthulu-workflows/<workspace>/<workflow-name>/
        let mut ws_dir = state.data_dir.join("cthulu-workflows").join(ws);
        if let Some(ref wf_name) = workflow_name {
            ws_dir = ws_dir.join(wf_name);
        }
        Some(ws_dir.to_string_lossy().to_string())
    } else {
        working_dir_override.clone()
    };

    if let Some(ref dir_override) = resolved_override {
        // ── Workflow/workspace mode: no agent lookup ──
        working_dir = dir_override.clone();

        // Ensure the workflow directory exists
        let wf_dir = std::path::Path::new(&working_dir);
        let _ = std::fs::create_dir_all(wf_dir);

        // Write CLAUDE.md with workflow schema docs (only if not present)
        ensure_workflow_claude_md(wf_dir);

        // Write hook settings for file-change tracking (no agent-specific hooks)
        let empty_hooks = std::collections::HashMap::new();
        super::chat::write_hook_settings(
            &state.hook_socket_path,
            &working_dir,
            &session_id,
            &empty_hooks,
        );

        // Use acceptEdits permission mode — auto-accepts Read/Write/Edit
        // but still prompts for Bash, WebFetch, etc.
        cmd.arg("--permission-mode");
        cmd.arg("acceptEdits");

        // Check for session history
        let session_log_path = state
            .data_dir
            .join("session_logs")
            .join(format!("{}.jsonl", &session_id));
        let has_history = session_log_path.exists()
            && std::fs::metadata(&session_log_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false);

        if has_history {
            cmd.arg("--resume");
            cmd.arg(&session_id);
        } else {
            // Minimal system prompt — CLAUDE.md provides the detailed context
            let wf_label = if let (Some(ref ws), Some(ref name)) = (&workspace, &workflow_name) {
                format!("{ws}/{name}")
            } else if let Some(ref ws) = workspace {
                ws.clone()
            } else {
                working_dir.clone()
            };
            let sys_prompt = format!(
                "You are a workflow assistant for: {wf_label}\n\
                 Read CLAUDE.md in your working directory for the full workflow schema reference.",
            );
            cmd.arg("--system-prompt");
            cmd.arg(&sys_prompt);
        }
    } else {
        // ── Agent mode: look up agent config ──
        let agent = state
            .agent_repo
            .get(&agent_id)
            .await
            .ok_or_else(|| format!("Agent not found: {agent_id}"))?;

        // Determine working directory
        working_dir = agent
            .working_dir
            .clone()
            .unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()));

        // Check if session has history (for --resume vs new session)
        let session_log_path = state
            .data_dir
            .join("session_logs")
            .join(format!("{}.jsonl", &session_id));
        let has_history = session_log_path.exists()
            && std::fs::metadata(&session_log_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false);

        // Write hook settings (permissions, file change tracking)
        super::chat::write_hook_settings(
            &state.hook_socket_path,
            &working_dir,
            &session_id,
            &agent.hooks,
        );

        // Permissions / allowed tools
        if !agent.permissions.is_empty() {
            cmd.arg("--allowedTools");
            cmd.arg(agent.permissions.join(","));
        }

        // Sub-agents (JSON-encoded, single --agents flag)
        if !agent.subagents.is_empty() {
            if let Ok(agents_json) = serde_json::to_string(&agent.subagents) {
                cmd.arg("--agents");
                cmd.arg(agents_json);
            }
        }

        // Session handling: new vs resume
        if has_history {
            cmd.arg("--resume");
            cmd.arg(&session_id);
        } else {
            // Build system prompt for new sessions
            let mut sys_prompt = format!(
                "You are \"{agent_name}\", an AI assistant. \
                 Your working directory is: {working_dir}\n\
                 Be efficient: short answers, no preamble, batch tool calls when possible.",
                agent_name = agent.name,
            );
            if let Some(ref extra) = agent.append_system_prompt {
                if !extra.is_empty() {
                    sys_prompt.push_str(&format!("\n\n{extra}"));
                }
            }
            cmd.arg("--system-prompt");
            cmd.arg(&sys_prompt);
        }

        // Auto-permissions
        if agent.auto_permissions {
            cmd.arg("--dangerously-skip-permissions");
        }
    }

    // Working directory
    cmd.cwd(&working_dir);

    // Environment for full color support in xterm.js
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("FORCE_COLOR", "3");
    cmd.env("CLAUDECODE", "");

    // Create PTY pair
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to create PTY: {e}"))?;

    // Spawn child process in the PTY
    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn claude in PTY: {e}"))?;

    // Get reader and writer from master
    let mut reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to clone PTY reader: {e}"))?;

    let writer = pty_pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

    // Spawn background reader task
    let app_handle = app.clone();
    let sid = session_id.clone();
    let pty_sessions_ref = pty_state.sessions.clone();

    let reader_handle = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_handle.emit(&format!("pty-data-{}", sid), data);
                }
                Err(e) => {
                    eprintln!("PTY reader error for session {}: {}", sid, e);
                    break;
                }
            }
        }
        // PTY closed — child exited
        let _ = app_handle.emit(
            &format!("pty-exit-{}", sid),
            json!({ "session_id": sid }),
        );
        // Clean up session from map
        let sessions_ref = pty_sessions_ref.clone();
        let sid_owned = sid.clone();
        tokio::spawn(async move {
            let mut sessions = sessions_ref.lock().await;
            sessions.remove(&sid_owned);
        });
    });

    // Store PTY session
    let pty_session = PtySession {
        writer: Arc::new(Mutex::new(writer)),
        master: Arc::new(Mutex::new(pty_pair.master)),
        child: Arc::new(Mutex::new(child)),
        reader_handle,
    };

    {
        let mut sessions = pty_state.sessions.lock().await;
        sessions.insert(session_id.clone(), pty_session);
    }

    Ok(json!({ "session_id": session_id }))
}

#[tauri::command]
pub async fn write_pty(
    pty_state: tauri::State<'_, PtyState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let sessions = pty_state.sessions.lock().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("No PTY session found for {}", session_id))?;

    let mut writer = session.writer.lock().await;
    writer
        .write_all(data.as_bytes())
        .map_err(|e| format!("Failed to write to PTY: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("Failed to flush PTY: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn resize_pty(
    pty_state: tauri::State<'_, PtyState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    let sessions = pty_state.sessions.lock().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("No PTY session found for {}", session_id))?;

    let master = session.master.lock().await;
    master
        .resize(PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to resize PTY: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn kill_pty(
    pty_state: tauri::State<'_, PtyState>,
    session_id: String,
) -> Result<(), String> {
    let mut sessions = pty_state.sessions.lock().await;
    if let Some(session) = sessions.remove(&session_id) {
        {
            let mut child = session.child.lock().await;
            let _ = child.kill();
        }
        session.reader_handle.abort();
    }
    Ok(())
}

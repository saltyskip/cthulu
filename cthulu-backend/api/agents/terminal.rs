use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use std::io::{Read, Write};
use tokio::sync::broadcast;

use crate::api::{AppState, PtyProcess};

/// Build the PTY pool key. If a session_id is provided, scope to that session;
/// otherwise fall back to agent-level key for backward compat.
pub(crate) fn pty_key(agent_id: &str, session_id: Option<&str>) -> String {
    match session_id {
        Some(sid) => format!("agent::{agent_id}::session::{sid}"),
        None => format!("agent::{agent_id}"),
    }
}

#[derive(Deserialize)]
pub(crate) struct TerminalQuery {
    pub session_id: Option<String>,
}

/// JSON message sent from the frontend for resize events.
#[derive(Deserialize)]
struct ResizeMessage {
    #[serde(rename = "type")]
    msg_type: String,
    cols: u16,
    rows: u16,
}

/// Spawn a Claude Code process inside a PTY and start a persistent reader task.
fn spawn_pty_claude(
    agent: &crate::agents::Agent,
    session_id: &str,
    is_new: bool,
    cols: u16,
    rows: u16,
) -> anyhow::Result<PtyProcess> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let working_dir = agent.working_dir.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| ".".into())
            .to_string_lossy()
            .to_string()
    });

    let mut cmd = CommandBuilder::new("claude");

    // Permissions
    if agent.permissions.is_empty() {
        cmd.arg("--dangerously-skip-permissions");
    } else {
        cmd.arg("--allowedTools");
        cmd.arg(agent.permissions.join(","));
    }

    // Session management
    if is_new {
        cmd.arg("--session-id");
        cmd.arg(session_id);

        // System prompt for new sessions
        if let Some(ref sys_prompt) = agent.append_system_prompt {
            if !sys_prompt.is_empty() {
                cmd.arg("--system-prompt");
                cmd.arg(sys_prompt);
            }
        }
    } else {
        cmd.arg("--resume");
        cmd.arg(session_id);
    }

    cmd.cwd(&working_dir);
    cmd.env("CLAUDECODE", "");

    let child = pair.slave.spawn_command(cmd)?;

    // Take writer once at spawn time — can only be taken once.
    let writer = pair.master.take_writer()?;

    // Clone reader once and start a persistent reader task.
    // All WS connections subscribe to the broadcast channel instead of cloning readers.
    let reader = pair.master.try_clone_reader()?;
    let (output_tx, _) = broadcast::channel::<Vec<u8>>(256);
    let tx_for_reader = output_tx.clone();

    tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // If no subscribers, send returns Err — that's fine, data is dropped.
                    let _ = tx_for_reader.send(buf[..n].to_vec());
                }
                Err(_) => break,
            }
        }
    });

    Ok(PtyProcess {
        master: pair.master,
        child,
        session_id: session_id.to_string(),
        writer: std::sync::Arc::new(std::sync::Mutex::new(writer)),
        output_tx,
    })
}

/// GET /agents/{id}/terminal — WebSocket upgrade for PTY passthrough.
pub(crate) async fn terminal_ws(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<TerminalQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal(socket, state, id, query.session_id))
}

/// Bidirectional bridge: PTY <-> WebSocket.
async fn handle_terminal(socket: WebSocket, state: AppState, agent_id: String, query_session_id: Option<String>) {
    use futures_util::{SinkExt, StreamExt};

    // Look up the agent
    let agent = match state.agent_repo.get(&agent_id).await {
        Some(a) => a,
        None => {
            tracing::error!(agent_id = %agent_id, "agent not found for terminal");
            return;
        }
    };

    // Agent-level key for session storage (always agent::{id})
    let agent_session_key = format!("agent::{agent_id}");

    // Resolve or create session (reuse existing active session)
    let (session_id, is_new) = {
        let mut all_sessions = state.interact_sessions.write().await;
        let flow_sessions = all_sessions
            .entry(agent_session_key.clone())
            .or_insert_with(|| {
                let sid = uuid::Uuid::new_v4().to_string();
                crate::api::FlowSessions {
                    flow_name: agent.name.clone(),
                    active_session: sid.clone(),
                    sessions: vec![crate::api::InteractSession {
                        session_id: sid,
                        summary: String::new(),
                        node_id: None,
                        working_dir: agent.working_dir.clone().unwrap_or_else(|| ".".into()),
                        active_pid: None,
                        busy: false,
                        message_count: 0,
                        total_cost: 0.0,
                        created_at: chrono::Utc::now().to_rfc3339(),
                        skills_dir: None,
                        kind: "interactive".to_string(),
                        flow_run: None,
                    }],
                }
            });

        // If a specific session_id was requested via query param, use it;
        // otherwise fall back to the active session.
        let target_sid = query_session_id
            .unwrap_or_else(|| flow_sessions.active_session.clone());
        let session = flow_sessions.get_session(&target_sid);
        let is_new = session.map(|s| s.message_count == 0).unwrap_or(true);

        (target_sid, is_new)
    };

    // Build session-scoped PTY pool key
    let key = pty_key(&agent_id, Some(&session_id));

    // Get or spawn PTY process
    let needs_spawn = {
        let pool = state.pty_processes.lock().await;
        !pool.contains_key(&key)
    };

    if needs_spawn {
        tracing::info!(
            key = %key,
            session_id = %session_id,
            is_new,
            "spawning PTY claude for agent terminal"
        );

        match spawn_pty_claude(&agent, &session_id, is_new, 120, 40) {
            Ok(pty) => {
                let mut pool = state.pty_processes.lock().await;
                pool.insert(key.clone(), pty);
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to spawn PTY claude");
                return;
            }
        }
    }

    // Get writer (shared Arc) and subscribe to output broadcast from the PTY
    let (writer, mut output_rx) = {
        let pool = state.pty_processes.lock().await;
        let pty = match pool.get(&key) {
            Some(p) => p,
            None => {
                tracing::error!("PTY process disappeared from pool");
                return;
            }
        };
        (pty.writer.clone(), pty.output_tx.subscribe())
    };

    let (mut ws_sink, mut ws_stream) = socket.split();

    // PTY output broadcast -> WS sink
    let ws_write_handle = tokio::spawn(async move {
        loop {
            match output_rx.recv().await {
                Ok(data) => {
                    if ws_sink.send(Message::Binary(data.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "terminal WS subscriber lagged");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // WS -> PTY writer (handle binary input + text resize messages)
    let pty_processes_ref = state.pty_processes.clone();
    let key_for_write = key.clone();

    while let Some(Ok(msg)) = ws_stream.next().await {
        match msg {
            Message::Binary(data) => {
                let mut w = writer.lock().unwrap();
                if w.write_all(&data).is_err() {
                    break;
                }
                let _ = w.flush();
            }
            Message::Text(text) => {
                // Try to parse as resize message
                if let Ok(resize) = serde_json::from_str::<ResizeMessage>(&text) {
                    if resize.msg_type == "resize" {
                        let pool = pty_processes_ref.lock().await;
                        if let Some(pty) = pool.get(&key_for_write) {
                            let _ = pty.master.resize(PtySize {
                                rows: resize.rows,
                                cols: resize.cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                    }
                } else {
                    // Plain text input — write as bytes
                    let mut w = writer.lock().unwrap();
                    if w.write_all(text.as_bytes()).is_err() {
                        break;
                    }
                    let _ = w.flush();
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // WS disconnected — PTY process stays alive for reconnect.
    // Just abort the WS forwarding task; the persistent reader task keeps running.
    ws_write_handle.abort();

    tracing::info!(key = %key, "terminal WebSocket disconnected (PTY stays alive)");
}

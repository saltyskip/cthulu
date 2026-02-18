use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::server::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SLACK_MAX_CHARS: usize = 4000;
const MAX_CHUNKS: usize = 20;
const CHUNK_DELAY_MS: u64 = 300;
const DEDUP_RING_SIZE: usize = 500;
const CLAUDE_TIMEOUT_SECS: u64 = 15 * 60;

// ---------------------------------------------------------------------------
// Thread session types
// ---------------------------------------------------------------------------

pub struct ThreadSession {
    pub session_id: String,
    pub active_pid: Option<u32>,
    pub busy: bool,
    pub message_count: u64,
    pub total_cost: f64,
}

pub type ThreadSessions = Arc<RwLock<HashMap<String, ThreadSession>>>;

pub fn new_sessions() -> ThreadSessions {
    Arc::new(RwLock::new(HashMap::new()))
}

// ---------------------------------------------------------------------------
// Resolve bot user ID via auth.test
// ---------------------------------------------------------------------------

pub async fn resolve_bot_user_id(state: &AppState) {
    let bot_token = match state.config.slack_bot_token() {
        Some(t) => t,
        None => {
            tracing::warn!("No SLACK_BOT_TOKEN set, cannot resolve bot user ID");
            return;
        }
    };

    let resp = state
        .http_client
        .post("https://slack.com/api/auth.test")
        .header("Authorization", format!("Bearer {bot_token}"))
        .send()
        .await;

    match resp {
        Ok(r) => {
            if let Ok(body) = r.json::<Value>().await {
                if body["ok"].as_bool() == Some(true) {
                    if let Some(user_id) = body["user_id"].as_str() {
                        tracing::info!(bot_user_id = user_id, "Resolved bot user ID");
                        *state.bot_user_id.write().await = Some(user_id.to_string());
                        return;
                    }
                }
                tracing::error!("auth.test failed: {}", body);
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to call auth.test");
        }
    }
}

// ---------------------------------------------------------------------------
// Handle incoming Slack event (called from slack_socket)
// ---------------------------------------------------------------------------

pub async fn handle_slack_event(state: AppState, event_json: Value) {
    // Extract event_id for dedup
    let event_id = event_json["event_id"]
        .as_str()
        .or_else(|| event_json["event"]["client_msg_id"].as_str())
        .unwrap_or("")
        .to_string();

    // Dedup check
    if !event_id.is_empty() {
        let mut seen = state.seen_event_ids.write().await;
        if seen.contains(&event_id) {
            tracing::debug!(event_id, "Duplicate event, skipping");
            return;
        }
        seen.push_back(event_id.clone());
        if seen.len() > DEDUP_RING_SIZE {
            seen.pop_front();
        }
    }

    let event = &event_json["event"];

    // Bot filter: skip if bot_id present
    if event.get("bot_id").is_some() && event["bot_id"].as_str().is_some() {
        tracing::debug!("Skipping bot message");
        return;
    }

    // Bot filter: skip if subtype present (message_changed, etc.)
    if event.get("subtype").is_some() && event["subtype"].as_str().is_some() {
        tracing::debug!("Skipping message with subtype");
        return;
    }

    let event_type = event["type"].as_str().unwrap_or("");
    if event_type != "message" && event_type != "app_mention" {
        tracing::debug!(event_type, "Ignoring non-message event type");
        return;
    }

    let user = event["user"].as_str().unwrap_or("").to_string();
    let channel = event["channel"].as_str().unwrap_or("").to_string();
    let text = event["text"].as_str().unwrap_or("").to_string();
    let ts = event["ts"].as_str().unwrap_or("").to_string();
    let thread_ts = event["thread_ts"]
        .as_str()
        .unwrap_or(&ts)
        .to_string();

    // Bot filter: skip if sender is our bot
    {
        let bot_id = state.bot_user_id.read().await;
        if let Some(ref bid) = *bot_id {
            if user == *bid {
                tracing::debug!("Skipping own message");
                return;
            }
        }
    }

    // Determine if we should respond
    let is_dm = channel.starts_with('D');
    let is_app_mention = event_type == "app_mention";

    let bot_mentioned = {
        let bot_id = state.bot_user_id.read().await;
        bot_id
            .as_ref()
            .map(|bid| text.contains(&format!("<@{bid}>")))
            .unwrap_or(false)
    };

    let has_existing_session = {
        let sessions = state.thread_sessions.read().await;
        sessions.contains_key(&thread_ts)
    };

    if !is_dm && !is_app_mention && !bot_mentioned && !has_existing_session {
        tracing::debug!("Message not directed at bot, ignoring");
        return;
    }

    tracing::info!(
        channel,
        user,
        thread_ts,
        "Processing Slack message"
    );

    if let Err(e) = handle_message(state, &channel, &thread_ts, &text).await {
        tracing::error!(error = %e, "Failed to handle message");
    }
}

// ---------------------------------------------------------------------------
// Handle message: parse commands or relay to Claude
// ---------------------------------------------------------------------------

async fn handle_message(
    state: AppState,
    channel: &str,
    thread_ts: &str,
    raw_text: &str,
) -> Result<()> {
    // Strip bot mention prefix: <@UBOTID> hello -> hello
    let text = strip_bot_mention(raw_text, &state).await;
    let text = text.trim();

    if text.is_empty() {
        return Ok(());
    }

    // Parse hashtag commands
    let lower = text.to_lowercase();
    if lower.starts_with("#status") {
        return handle_status(&state, channel, thread_ts).await;
    }
    if lower.starts_with("#stop") {
        return handle_stop(&state, channel, thread_ts).await;
    }
    if lower.starts_with("#new") {
        return handle_new(&state, channel, thread_ts).await;
    }

    // Regular message -> relay to Claude
    relay_to_claude(state, channel, thread_ts, text).await
}

async fn strip_bot_mention(text: &str, state: &AppState) -> String {
    let bot_id = state.bot_user_id.read().await;
    let mut result = text.to_string();
    if let Some(ref bid) = *bot_id {
        let mention = format!("<@{bid}>");
        result = result.replace(&mention, "");
    }
    // Also strip any generic <@...> at the start
    if result.starts_with("<@") {
        if let Some(end) = result.find('>') {
            result = result[end + 1..].to_string();
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Relay message to Claude
// ---------------------------------------------------------------------------

async fn relay_to_claude(
    state: AppState,
    channel: &str,
    thread_ts: &str,
    text: &str,
) -> Result<()> {
    let bot_token = state
        .config
        .slack_bot_token()
        .context("SLACK_BOT_TOKEN not set")?;

    // Check if session is busy
    {
        let sessions = state.thread_sessions.read().await;
        if let Some(session) = sessions.get(thread_ts) {
            if session.busy {
                reply(
                    &state.http_client,
                    &bot_token,
                    channel,
                    thread_ts,
                    "_Processing previous message... please wait._",
                )
                .await?;
                return Ok(());
            }
        }
    }

    // Create or get session
    let (session_id, is_new) = {
        let mut sessions = state.thread_sessions.write().await;
        let entry = sessions.entry(thread_ts.to_string()).or_insert_with(|| {
            let sid = uuid::Uuid::new_v4().to_string();
            tracing::info!(session_id = %sid, thread_ts, "Creating new Claude session");
            ThreadSession {
                session_id: sid,
                active_pid: None,
                busy: false,
                message_count: 0,
                total_cost: 0.0,
            }
        });
        entry.busy = true;
        (entry.session_id.clone(), entry.message_count == 0)
    };

    // Build Claude command
    let mut args = vec![
        "--print".to_string(),
        "--verbose".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--dangerously-skip-permissions".to_string(),
    ];

    if is_new {
        args.push("--session-id".to_string());
        args.push(session_id.clone());
    } else {
        args.push("--resume".to_string());
        args.push(session_id.clone());
    }

    args.push("-".to_string()); // read from stdin

    tracing::info!(
        session_id,
        is_new,
        "Spawning Claude process"
    );

    let spawn_result = Command::new("claude")
        .args(&args)
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawn_result {
        Ok(c) => c,
        Err(e) => {
            let err_msg = format!("Failed to spawn Claude: {e}");
            tracing::error!("{err_msg}");
            reset_session_busy(&state, thread_ts).await;
            reply(&state.http_client, &bot_token, channel, thread_ts, &err_msg).await?;
            return Ok(());
        }
    };

    // Store PID for #stop
    if let Some(pid) = child.id() {
        let mut sessions = state.thread_sessions.write().await;
        if let Some(session) = sessions.get_mut(thread_ts) {
            session.active_pid = Some(pid);
        }
    }

    // Write prompt to stdin, then close
    {
        let mut stdin = child.stdin.take().expect("stdin piped");
        if let Err(e) = stdin.write_all(text.as_bytes()).await {
            tracing::error!(error = %e, "Failed to write to Claude stdin");
            let _ = child.kill().await;
            reset_session_busy(&state, thread_ts).await;
            reply(
                &state.http_client,
                &bot_token,
                channel,
                thread_ts,
                &format!("Failed to write to Claude: {e}"),
            )
            .await?;
            return Ok(());
        }
        // stdin drops here, closing the pipe
    }

    // Stream stderr to tracing
    let stderr = child.stderr.take().expect("stderr piped");
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if !line.is_empty() {
                tracing::debug!(source = "claude-relay-stderr", "{}", line);
            }
        }
    });

    // Parse stdout stream-json
    let stdout = child.stdout.take().expect("stdout piped");
    let stdout_handle = tokio::spawn(async move {
        let mut response_text = String::new();
        let mut cost: f64 = 0.0;

        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<Value>(&line) {
                let event_type = event
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                match event_type {
                    "assistant" => {
                        if let Some(content) = event
                            .get("message")
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_array())
                        {
                            for block in content {
                                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                                        response_text.push_str(t);
                                    }
                                }
                            }
                        }
                    }
                    "result" => {
                        cost = event
                            .get("total_cost_usd")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        // Fallback: use result text if no assistant text captured
                        if response_text.is_empty() {
                            if let Some(t) = event.get("result").and_then(|v| v.as_str()) {
                                response_text = t.to_string();
                            }
                        }
                        tracing::info!(
                            cost_usd = cost,
                            "Claude relay finished"
                        );
                    }
                    "system" => {
                        tracing::debug!("Claude system event");
                    }
                    _ => {}
                }
            }
        }

        (response_text, cost)
    });

    // Wait for process with timeout
    let status = match tokio::time::timeout(
        std::time::Duration::from_secs(CLAUDE_TIMEOUT_SECS),
        child.wait(),
    )
    .await
    {
        Ok(result) => result.context("failed to wait on claude")?,
        Err(_) => {
            tracing::error!("Claude relay process timed out");
            let _ = child.kill().await;
            reset_session_busy(&state, thread_ts).await;
            reply(
                &state.http_client,
                &bot_token,
                channel,
                thread_ts,
                "_Claude timed out after 15 minutes._",
            )
            .await?;
            return Ok(());
        }
    };

    let (response_text, cost) = stdout_handle.await.unwrap_or((String::new(), 0.0));

    // Update session state
    {
        let mut sessions = state.thread_sessions.write().await;
        if let Some(session) = sessions.get_mut(thread_ts) {
            session.busy = false;
            session.active_pid = None;
            session.message_count += 1;
            session.total_cost += cost;
        }
    }

    let response = if response_text.is_empty() {
        if status.success() {
            "_(No response from Claude)_".to_string()
        } else {
            format!("_Claude exited with status: {status}_")
        }
    } else {
        response_text
    };

    tracing::info!(
        len = response.len(),
        "Sending Claude response to Slack"
    );

    send_chunked_reply(&state.http_client, &bot_token, channel, thread_ts, &response).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Hashtag commands
// ---------------------------------------------------------------------------

async fn handle_status(state: &AppState, channel: &str, thread_ts: &str) -> Result<()> {
    let bot_token = state
        .config
        .slack_bot_token()
        .context("SLACK_BOT_TOKEN not set")?;

    let sessions = state.thread_sessions.read().await;
    let msg = if let Some(session) = sessions.get(thread_ts) {
        format!(
            "*Session Status*\n\
             Session ID: `{}`\n\
             Messages: {}\n\
             Total cost: ${:.4}\n\
             Busy: {}",
            session.session_id,
            session.message_count,
            session.total_cost,
            if session.busy { "yes" } else { "no" }
        )
    } else {
        "No active session in this thread.".to_string()
    };

    reply(&state.http_client, &bot_token, channel, thread_ts, &msg).await
}

async fn handle_stop(state: &AppState, channel: &str, thread_ts: &str) -> Result<()> {
    let bot_token = state
        .config
        .slack_bot_token()
        .context("SLACK_BOT_TOKEN not set")?;

    let pid = {
        let mut sessions = state.thread_sessions.write().await;
        if let Some(session) = sessions.get_mut(thread_ts) {
            let pid = session.active_pid.take();
            session.busy = false;
            pid
        } else {
            None
        }
    };

    if let Some(pid) = pid {
        kill_process_group(pid);
        reply(
            &state.http_client,
            &bot_token,
            channel,
            thread_ts,
            "_Stopped Claude process._",
        )
        .await
    } else {
        reply(
            &state.http_client,
            &bot_token,
            channel,
            thread_ts,
            "_No running process to stop._",
        )
        .await
    }
}

async fn handle_new(state: &AppState, channel: &str, thread_ts: &str) -> Result<()> {
    let bot_token = state
        .config
        .slack_bot_token()
        .context("SLACK_BOT_TOKEN not set")?;

    // Kill any running process and remove the session
    let pid = {
        let mut sessions = state.thread_sessions.write().await;
        let pid = sessions
            .get(thread_ts)
            .and_then(|s| s.active_pid);
        sessions.remove(thread_ts);
        pid
    };

    if let Some(pid) = pid {
        kill_process_group(pid);
    }

    reply(
        &state.http_client,
        &bot_token,
        channel,
        thread_ts,
        "_Session reset. Next message will start a fresh conversation._",
    )
    .await
}

// ---------------------------------------------------------------------------
// Process management
// ---------------------------------------------------------------------------

fn kill_process_group(pid: u32) {
    #[cfg(unix)]
    {
        use std::process::Command as StdCommand;
        // Try SIGTERM on the process group
        let _ = StdCommand::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .status();
        // Give it a moment
        std::thread::sleep(std::time::Duration::from_millis(500));
        // Force kill
        let _ = StdCommand::new("kill")
            .args(["-KILL", &format!("-{pid}")])
            .status();
    }
    #[cfg(not(unix))]
    {
        tracing::warn!(pid, "Process kill not implemented on this platform");
    }
}

async fn reset_session_busy(state: &AppState, thread_ts: &str) {
    let mut sessions = state.thread_sessions.write().await;
    if let Some(session) = sessions.get_mut(thread_ts) {
        session.busy = false;
        session.active_pid = None;
    }
}

// ---------------------------------------------------------------------------
// Slack messaging
// ---------------------------------------------------------------------------

async fn reply(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    thread_ts: &str,
    text: &str,
) -> Result<()> {
    let body = json!({
        "channel": channel,
        "thread_ts": thread_ts,
        "text": text,
    });

    let resp = client
        .post("https://slack.com/api/chat.postMessage")
        .header("Authorization", format!("Bearer {bot_token}"))
        .json(&body)
        .send()
        .await
        .context("Failed to post Slack message")?;

    let status = resp.status();
    let resp_body: Value = resp.json().await.unwrap_or_default();

    if !status.is_success() || resp_body["ok"].as_bool() != Some(true) {
        let err = resp_body["error"].as_str().unwrap_or("unknown");
        tracing::error!(error = err, "Slack chat.postMessage failed");
    }

    Ok(())
}

async fn send_chunked_reply(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    thread_ts: &str,
    text: &str,
) -> Result<()> {
    // If it fits in one message, send directly
    if text.len() <= SLACK_MAX_CHARS {
        return reply(client, bot_token, channel, thread_ts, text).await;
    }

    let mut remaining = text;
    let mut chunk_count = 0;

    while !remaining.is_empty() && chunk_count < MAX_CHUNKS {
        let chunk = if remaining.len() <= SLACK_MAX_CHARS {
            remaining
        } else {
            let window = &remaining[..SLACK_MAX_CHARS];
            // Try to split on last newline
            if let Some(pos) = window.rfind('\n') {
                &remaining[..pos]
            // Try to split on last space
            } else if let Some(pos) = window.rfind(' ') {
                &remaining[..pos]
            // Hard cut
            } else {
                window
            }
        };

        reply(client, bot_token, channel, thread_ts, chunk).await?;

        remaining = &remaining[chunk.len()..];
        // Skip the split character if it was a newline or space
        if remaining.starts_with('\n') || remaining.starts_with(' ') {
            remaining = &remaining[1..];
        }

        chunk_count += 1;

        if !remaining.is_empty() && chunk_count < MAX_CHUNKS {
            tokio::time::sleep(std::time::Duration::from_millis(CHUNK_DELAY_MS)).await;
        }
    }

    if !remaining.is_empty() {
        reply(
            client,
            bot_token,
            channel,
            thread_ts,
            "_(Response truncated -- too long for Slack)_",
        )
        .await?;
    }

    Ok(())
}

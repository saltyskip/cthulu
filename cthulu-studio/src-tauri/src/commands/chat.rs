use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use tauri::Emitter;

use cthulu::api::{AppState, LiveClaudeProcess};

// ---------------------------------------------------------------------------
// Chat request types
// ---------------------------------------------------------------------------

// AgentChatRequest struct removed — agent_chat now uses flat params for Tauri IPC compatibility.

#[derive(Deserialize)]
pub struct ImageAttachment {
    pub media_type: String,
    pub data: String, // base64-encoded
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Truncate a string to ~80 chars for use as a session summary.
fn make_summary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= 80 {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(80).collect();
    let boundary = truncated.rfind(' ').unwrap_or(truncated.len());
    format!("{}...", &truncated[..boundary])
}

/// Write `.claude/settings.local.json` with hook configuration for this session.
/// Desktop-only: uses command-type hooks via the cthulu-hook.sh helper script.
pub(crate) fn write_hook_settings(
    hook_socket_path: &Option<std::path::PathBuf>,
    working_dir: &str,
    session_id: &str,
    agent_hooks: &std::collections::HashMap<String, Vec<cthulu::agents::AgentHookGroup>>,
) {
    let claude_dir = std::path::Path::new(working_dir).join(".claude");
    let _ = std::fs::create_dir_all(&claude_dir);
    let settings_path = claude_dir.join("settings.local.json");

    let mut settings: serde_json::Value = if settings_path.exists() {
        std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}))
    } else {
        json!({})
    };

    let mut hooks_map = serde_json::Map::new();
    let sid = session_id;

    if hook_socket_path.is_some() {
        let script = std::env::temp_dir().join("cthulu-hook.sh");
        let sp = script.display().to_string();
        hooks_map.insert(
            "PreToolUse".into(),
            json!([{
                "matcher": "Write|Edit|MultiEdit|NotebookEdit|Bash",
                "hooks": [{ "type": "command", "command": format!("{sp} pre-tool-use {sid}"), "timeout": 130 }]
            }]),
        );
        hooks_map.insert(
            "PostToolUse".into(),
            json!([{
                "matcher": "Write|Edit|MultiEdit|NotebookEdit|Bash",
                "hooks": [{ "type": "command", "command": format!("{sp} post-tool-use {sid}") }]
            }]),
        );
        hooks_map.insert(
            "Stop".into(),
            json!([{
                "hooks": [{ "type": "command", "command": format!("{sp} stop {sid}") }]
            }]),
        );
    }

    // Merge per-agent hooks: append agent hook groups after system groups
    for (event, agent_groups) in agent_hooks {
        let groups_json = serde_json::to_value(agent_groups).unwrap_or(json!([]));
        if let Some(existing) = hooks_map.get_mut(event) {
            if let Some(arr) = existing.as_array_mut() {
                if let Some(extra) = groups_json.as_array() {
                    arr.extend(extra.iter().cloned());
                }
            }
        } else {
            hooks_map.insert(event.clone(), groups_json);
        }
    }

    settings["hooks"] = Value::Object(hooks_map);
    if let Ok(json_str) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::write(&settings_path, json_str);
        tracing::info!(
            path = %settings_path.display(),
            session_id = %sid,
            "wrote .claude/settings.local.json with hooks"
        );
    }
}

/// Parse a raw Claude stdout line into a list of (event_type, data_json) pairs
/// for broadcasting. Copied from cthulu-backend/api/agents/chat.rs.
fn parse_claude_line_to_sse_events(line: &str) -> Vec<(String, String)> {
    let mut events = Vec::new();

    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(line) {
        let event_type = json_val
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        match event_type {
            "system" => {
                // Skip system events on resume
            }
            "content_block_delta" => {
                if let Some(delta) = json_val.get("delta") {
                    let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if delta_type == "text_delta" {
                        let text = delta.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        if !text.is_empty() {
                            events.push((
                                "text".to_string(),
                                serde_json::to_string(&json!({"text": text})).unwrap(),
                            ));
                        }
                    }
                }
            }
            "content_block_start" => {
                if let Some(content_block) = json_val.get("content_block") {
                    let block_type = content_block
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if block_type == "tool_use" {
                        let tool = content_block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        events.push((
                            "tool_use".to_string(),
                            serde_json::to_string(&json!({"tool": tool, "input": ""})).unwrap(),
                        ));
                    }
                }
            }
            "assistant" => {
                if let Some(content) = json_val
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                {
                    for block in content {
                        let block_type =
                            block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match block_type {
                            "tool_use" => {
                                let tool = block
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                let input = block
                                    .get("input")
                                    .map(|v| {
                                        if v.is_string() {
                                            v.as_str().unwrap_or("").to_string()
                                        } else {
                                            serde_json::to_string(v).unwrap_or_default()
                                        }
                                    })
                                    .unwrap_or_default();
                                events.push((
                                    "tool_use".to_string(),
                                    serde_json::to_string(
                                        &json!({"tool": tool, "input": input}),
                                    )
                                    .unwrap(),
                                ));
                            }
                            "tool_result" => {
                                let result_content = block
                                    .get("content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                events.push((
                                    "tool_result".to_string(),
                                    serde_json::to_string(
                                        &json!({"content": result_content}),
                                    )
                                    .unwrap(),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
            "result" => {
                let cost = json_val
                    .get("total_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let turns = json_val
                    .get("num_turns")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let result_text = json_val
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                events.push((
                    "result".to_string(),
                    serde_json::to_string(
                        &json!({"text": result_text, "cost": cost, "turns": turns}),
                    )
                    .unwrap(),
                ));
            }
            _ => {}
        }
    } else {
        events.push((
            "text".to_string(),
            serde_json::to_string(&json!({"text": line})).unwrap(),
        ));
    }

    events
}

// ---------------------------------------------------------------------------
// Agent chat — full implementation
// ---------------------------------------------------------------------------
//
// Spawns the `claude` CLI process, writes the user prompt to stdin, then
// spawns a background tokio task that reads stdout/stderr and emits Tauri
// events. Returns `{ session_id }` immediately.

/// Stale busy timeout — sessions busy longer than this are auto-recovered.
const STALE_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[tauri::command]
pub async fn agent_chat(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    prompt: String,
    session_id: Option<String>,
    flow_id: Option<String>,
    node_id: Option<String>,
    images: Option<Vec<ImageAttachment>>,
) -> Result<Value, String> {
    eprintln!("[AGENT_CHAT] >>> ENTERED agent_chat command: agent_id={}, prompt_len={}, session_id={:?}", agent_id, prompt.len(), session_id);
    crate::wait_ready(&ready).await?;
    eprintln!("[AGENT_CHAT] >>> past wait_ready");

    // 1. Validate agent exists
    let agent = state
        .agent_repo
        .get(&agent_id)
        .await
        .ok_or_else(|| "agent not found".to_string())?;

    let key = format!("agent::{agent_id}");
    let _ = (flow_id, node_id); // reserved for future use
    let images = images.unwrap_or_default();

    // 2. Resolve target session
    let target_session_id = if let Some(ref sid) = session_id {
        let sessions = state.interact_sessions.read().await;
        if let Some(flow_sessions) = sessions.get(&key) {
            if flow_sessions.get_session(sid).is_none() {
                return Err("session not found".to_string());
            }
        } else {
            return Err("no sessions for this agent".to_string());
        }
        sid.clone()
    } else {
        let sessions = state.interact_sessions.read().await;
        sessions
            .get(&key)
            .map(|fs| fs.active_session.clone())
            .unwrap_or_default()
    };

    if target_session_id.is_empty() {
        return Err("no active session — create a session first".to_string());
    }

    // 3. Check busy, mark busy, extract session state
    let (is_new, working_dir) = {
        let mut all_sessions = state.interact_sessions.write().await;
        let flow_sessions = all_sessions
            .get_mut(&key)
            .ok_or_else(|| "no sessions for this agent".to_string())?;

        let session = flow_sessions
            .get_session_mut(&target_session_id)
            .ok_or_else(|| "session not found".to_string())?;

        if session.busy {
            // Check for stale busy — process might be dead
            let proc_k = format!("agent::{agent_id}::session::{}", session.session_id);
            let is_stale = {
                let mut pool = state.live_processes.lock().await;
                if let Some(proc) = pool.get_mut(&proc_k) {
                    matches!(proc.child.try_wait(), Ok(Some(_)))
                } else {
                    session
                        .busy_since
                        .map(|since| {
                            chrono::Utc::now()
                                .signed_duration_since(since)
                                .to_std()
                                .unwrap_or_default()
                                > STALE_BUSY_TIMEOUT
                        })
                        .unwrap_or(true)
                }
            };

            if is_stale {
                tracing::warn!(
                    session_id = %session.session_id,
                    "auto-recovering stale busy session"
                );
                let mut pool = state.live_processes.lock().await;
                pool.remove(&proc_k);
                session.busy = false;
                session.busy_since = None;
                session.active_pid = None;
            } else {
                return Err(
                    "session is busy processing a previous message".to_string(),
                );
            }
        }

        let is_new = session.message_count == 0;

        if is_new && session.summary.is_empty() {
            session.summary = make_summary(&prompt);
        }

        session.busy = true;
        session.busy_since = Some(chrono::Utc::now());
        let wdir = session.working_dir.clone();

        flow_sessions.active_session = target_session_id.clone();

        let sessions_snapshot = all_sessions.clone();
        drop(all_sessions);
        state.save_sessions_to_disk(&sessions_snapshot);

        (is_new, wdir)
    };

    // 4. Write hook settings
    write_hook_settings(
        &state.hook_socket_path,
        &working_dir,
        &target_session_id,
        &agent.hooks,
    );

    // 5. Build system prompt for new sessions
    let system_prompt = if is_new {
        let mut sys_prompt = format!(
            "You are \"{agent_name}\", an AI assistant. \
             Your working directory is: {working_dir}\n\
             Be efficient: short answers, no preamble, batch tool calls when possible.",
            agent_name = agent.name,
            working_dir = working_dir,
        );
        if let Some(ref extra) = agent.append_system_prompt {
            if !extra.is_empty() {
                sys_prompt.push_str(&format!("\n\n{extra}"));
            }
        }
        Some(sys_prompt)
    } else {
        None
    };

    // 6. Spawn or reuse the Claude CLI process
    let proc_key = format!("agent::{agent_id}::session::{target_session_id}");
    let permissions = agent.permissions.clone();

    let spawn_result = {
        let mut pool = state.live_processes.lock().await;
        if pool.contains_key(&proc_key) {
            None // Already exists
        } else {
            let mut args = vec![
                "--verbose".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--input-format".to_string(),
                "stream-json".to_string(),
            ];

            if !permissions.is_empty() {
                args.push("--allowedTools".to_string());
                args.push(permissions.join(","));
            }

            if !agent.subagents.is_empty() {
                if let Ok(agents_json) = serde_json::to_string(&agent.subagents) {
                    args.push("--agents".to_string());
                    args.push(agents_json);
                    tracing::info!(
                        agent_id = %agent_id,
                        subagent_count = agent.subagents.len(),
                        "passing sub-agents to claude CLI"
                    );
                }
            }

            if is_new {
                args.push("--session-id".to_string());
                args.push(target_session_id.clone());
                if let Some(ref sys_prompt) = system_prompt {
                    args.push("--system-prompt".to_string());
                    args.push(sys_prompt.clone());
                }
            } else {
                args.push("--resume".to_string());
                args.push(target_session_id.clone());
            }

            tracing::info!(
                key = %proc_key,
                session_id = %target_session_id,
                is_new,
                "spawning persistent claude for agent chat"
            );

            match Command::new("claude")
                .args(&args)
                .current_dir(&working_dir)
                .env_remove("CLAUDECODE")
                .env("CLAUDECODE", "")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(mut child) => {
                    if let Some(pid) = child.id() {
                        let mut all_sessions = state.interact_sessions.write().await;
                        if let Some(fs) = all_sessions.get_mut(&key) {
                            if let Some(s) = fs.get_session_mut(&target_session_id) {
                                s.active_pid = Some(pid);
                            }
                        }
                    }

                    let child_stdin = child.stdin.take().expect("stdin piped");

                    let stdout = child.stdout.take().expect("stdout piped");
                    let (stdout_tx, stdout_rx) =
                        tokio::sync::mpsc::unbounded_channel::<String>();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stdout);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            if stdout_tx.send(line).is_err() {
                                break;
                            }
                        }
                    });

                    let stderr = child.stderr.take().expect("stderr piped");
                    let (stderr_tx, stderr_rx) =
                        tokio::sync::mpsc::unbounded_channel::<String>();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stderr);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            if !line.is_empty() {
                                let _ = stderr_tx.send(line);
                            }
                        }
                    });

                    let live_proc = LiveClaudeProcess {
                        stdin: child_stdin,
                        stdout_lines: stdout_rx,
                        stderr_lines: stderr_rx,
                        child,
                        busy: false,
                    };

                    pool.insert(proc_key.clone(), live_proc);
                    Some(Ok(()))
                }
                Err(e) => Some(Err(e)),
            }
        }
    };

    // Handle spawn result
    match spawn_result {
        Some(Err(e)) => {
            tracing::error!(error = %e, "failed to spawn claude for agent chat");
            let mut all_sessions = state.interact_sessions.write().await;
            if let Some(fs) = all_sessions.get_mut(&key) {
                if let Some(s) = fs.get_session_mut(&target_session_id) {
                    s.busy = false;
                    s.busy_since = None;
                }
            }
            return Err(format!("failed to spawn claude: {e}"));
        }
        Some(Ok(())) => {
            tracing::info!(proc_key = %proc_key, "session started");
        }
        None => {
            tracing::info!(proc_key = %proc_key, "reusing existing process");
        }
    }

    // 7. Write prompt to stdin (stream-json format)
    {
        let mut pool = state.live_processes.lock().await;
        if let Some(proc) = pool.get_mut(&proc_key) {
            // Build content: plain string when no images, content block array with images
            let content = if images.is_empty() {
                json!(prompt)
            } else {
                let mut blocks: Vec<Value> = Vec::new();
                if !prompt.trim().is_empty() {
                    blocks.push(json!({ "type": "text", "text": prompt }));
                }
                for img in &images {
                    blocks.push(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": img.media_type,
                            "data": img.data,
                        }
                    }));
                }
                json!(blocks)
            };
            let input_msg = serde_json::to_string(&json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": content,
                }
            }))
            .unwrap();
            let write_result = proc
                .stdin
                .write_all(format!("{input_msg}\n").as_bytes())
                .await;
            if let Err(e) = write_result {
                tracing::error!(error = %e, "failed to write to persistent claude stdin");
                pool.remove(&proc_key);
                let mut all_sessions = state.interact_sessions.write().await;
                if let Some(fs) = all_sessions.get_mut(&key) {
                    if let Some(s) = fs.get_session_mut(&target_session_id) {
                        s.busy = false;
                        s.busy_since = None;
                        s.active_pid = None;
                    }
                }
                return Err(format!(
                    "stdin write failed: {e}. Session will restart on next message."
                ));
            }
            proc.busy = true;
        } else {
            return Err("process not found in pool".to_string());
        }
    }

    // 8. Create broadcast channel + event buffer
    let (bc_tx, _) = broadcast::channel::<String>(1024);
    {
        let mut streams = state.session_streams.lock().await;
        streams.insert(proc_key.clone(), bc_tx.clone());
        tracing::info!(
            proc_key = %proc_key,
            total_streams = streams.len(),
            "created broadcast channel for agent chat"
        );
    }
    {
        let mut buffers = state.chat_event_buffers.lock().await;
        buffers.insert(proc_key.clone(), Vec::new());
    }

    // 9. Spawn background reader task
    {
        let bc_tx = bc_tx.clone();
        let live_processes = state.live_processes.clone();
        let sessions_ref = state.interact_sessions.clone();
        let session_streams = state.session_streams.clone();
        let chat_event_buffers = state.chat_event_buffers.clone();
        let proc_key = proc_key.clone();
        let key_for_bg = key.clone();
        let sid_for_bg = target_session_id.clone();
        let sessions_path = state.sessions_path.clone();
        let data_dir = state.data_dir.clone();
        let prompt_for_log = prompt.clone();
        let app_clone = app.clone();
        let event_channel = format!("chat-event-{sid_for_bg}");

        tokio::spawn(async move {
            tracing::info!(proc_key = %proc_key, "background reader task STARTED");
            let mut session_cost: f64 = 0.0;
            let mut event_count: u64 = 0;

            // Set up JSONL log file for session history persistence
            let logs_dir = data_dir.join("session_logs");
            let _ = std::fs::create_dir_all(&logs_dir);
            let log_path = logs_dir.join(format!("{sid_for_bg}.jsonl"));

            let append_log = |line: &str| {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                {
                    let _ = writeln!(f, "{}", line);
                }
            };

            // Log the user prompt that initiated this turn
            append_log(&format!(
                "user:{}",
                serde_json::to_string(&json!({"text": prompt_for_log})).unwrap_or_default()
            ));

            loop {
                let (line, stderr_batch) = {
                    let mut pool = live_processes.lock().await;
                    if let Some(proc) = pool.get_mut(&proc_key) {
                        let mut errs = Vec::new();
                        while let Ok(err_line) = proc.stderr_lines.try_recv() {
                            errs.push(err_line);
                        }
                        let stdout_line = proc.stdout_lines.try_recv().ok();
                        (stdout_line, errs)
                    } else {
                        break;
                    }
                };

                // Emit stderr lines
                for err_line in stderr_batch {
                    tracing::debug!(stderr = %err_line, "claude stderr");
                    let event_str = format!("stderr:{err_line}");
                    let _ = bc_tx.send(event_str.clone());
                    append_log(&event_str);
                    // Emit via Tauri event
                    let payload = json!({ "type": "stderr", "data": err_line });
                    let _ = app_clone.emit(&event_channel, &payload);
                    let mut buffers = chat_event_buffers.lock().await;
                    if let Some(buf) = buffers.get_mut(&proc_key) {
                        buf.push(event_str);
                    }
                }

                if let Some(line) = line {
                    if line.is_empty() {
                        continue;
                    }

                    let events = parse_claude_line_to_sse_events(&line);
                    let mut is_result = false;

                    for (event_type, data_json) in &events {
                        if event_type == "result" {
                            is_result = true;
                            if let Ok(val) =
                                serde_json::from_str::<serde_json::Value>(data_json)
                            {
                                session_cost = val
                                    .get("cost")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                            }
                        }
                        let event_str = format!("{event_type}:{data_json}");
                        event_count += 1;
                        let _ = bc_tx.send(event_str.clone());
                        tracing::debug!(
                            proc_key = %proc_key,
                            event_type = %event_type,
                            event_count,
                            "background task broadcast event"
                        );
                        append_log(&event_str);

                        // Emit via Tauri event — payload is a JSON value so the
                        // frontend can JSON.parse(event.payload) and get { type, data }
                        let payload = json!({ "type": event_type, "data": data_json });
                        let _ = app_clone.emit(&event_channel, &payload);

                        let mut buffers = chat_event_buffers.lock().await;
                        if let Some(buf) = buffers.get_mut(&proc_key) {
                            buf.push(event_str);
                        }
                    }

                    if is_result {
                        break;
                    }
                } else {
                    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

                    let mut pool = live_processes.lock().await;
                    if let Some(proc) = pool.get_mut(&proc_key) {
                        if let Ok(Some(_status)) = proc.child.try_wait() {
                            pool.remove(&proc_key);
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            // Mark session as not busy, update stats
            {
                let mut pool = live_processes.lock().await;
                if let Some(proc) = pool.get_mut(&proc_key) {
                    proc.busy = false;
                }
            }
            {
                let mut all_sessions = sessions_ref.write().await;
                if let Some(fs) = all_sessions.get_mut(&key_for_bg) {
                    if let Some(s) = fs.get_session_mut(&sid_for_bg) {
                        s.busy = false;
                        s.busy_since = None;
                        s.message_count += 1;
                        s.total_cost += session_cost;
                    }
                }
                let sessions_snapshot = all_sessions.clone();
                drop(all_sessions);
                cthulu::api::save_sessions(&sessions_path, &sessions_snapshot);
            }

            // Send done event
            tracing::info!(
                proc_key = %proc_key,
                event_count,
                "background reader task DONE, sending done event"
            );
            let done_data =
                serde_json::to_string(&json!({"exit_code": 0})).unwrap();
            let done_event = format!("done:{done_data}");
            let _ = bc_tx.send(done_event.clone());
            append_log(&done_event);
            {
                let payload = json!({ "type": "done", "data": done_data });
                let _ = app_clone.emit(&event_channel, &payload);
            }
            {
                let mut buffers = chat_event_buffers.lock().await;
                if let Some(buf) = buffers.get_mut(&proc_key) {
                    buf.push(done_event);
                }
            }

            // Clean up broadcast channel after delay for reconnects
            tracing::info!(proc_key = %proc_key, "waiting 5s before cleanup");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            {
                let mut streams = session_streams.lock().await;
                streams.remove(&proc_key);
                tracing::info!(
                    proc_key = %proc_key,
                    remaining_streams = streams.len(),
                    "cleaned up broadcast channel"
                );
            }
            {
                let mut buffers = chat_event_buffers.lock().await;
                buffers.remove(&proc_key);
                tracing::info!(
                    proc_key = %proc_key,
                    "cleaned up event buffer. Background task EXIT."
                );
            }
        });
    }

    // 10. Return session_id immediately — events stream via Tauri events
    Ok(json!({
        "session_id": target_session_id,
    }))
}

// ---------------------------------------------------------------------------
// Stop agent chat
// ---------------------------------------------------------------------------

// StopChatRequest struct removed — stop_agent_chat now uses flat params for Tauri IPC compatibility.

#[tauri::command]
pub async fn stop_agent_chat(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: Option<String>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let key = format!("agent::{agent_id}");

    let target_sid = {
        let sessions = state.interact_sessions.read().await;
        session_id
            .or_else(|| sessions.get(&key).map(|fs| fs.active_session.clone()))
    };

    let sid = target_sid.clone().unwrap_or_default();
    let proc_key = format!("agent::{agent_id}::session::{sid}");

    // Remove the persistent process from the pool (Drop impl will kill it)
    {
        let mut pool = state.live_processes.lock().await;
        pool.remove(&proc_key);
        pool.remove(&key);
    }

    // Disconnect SDK session if present
    {
        let mut sdk_pool = state.sdk_sessions.lock().await;
        if let Some(mut session) = sdk_pool.remove(&proc_key) {
            let _ = session.disconnect().await;
        }
    }

    // Clear busy flag
    let mut all_sessions = state.interact_sessions.write().await;
    if let Some(flow_sessions) = all_sessions.get_mut(&key) {
        let sid = target_sid.unwrap_or_else(|| flow_sessions.active_session.clone());
        if let Some(session) = flow_sessions.get_session_mut(&sid) {
            session.active_pid = None;
            session.busy = false;
            session.busy_since = None;
        }
    }

    Ok(json!({ "status": "stopped" }))
}

// ---------------------------------------------------------------------------
// Reconnect agent chat
// ---------------------------------------------------------------------------
//
// Replays buffered events from chat_event_buffers via Tauri events, then
// re-subscribes to the broadcast channel if the session is still active.

#[tauri::command]
pub async fn reconnect_agent_chat(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;

    let proc_key = format!("agent::{agent_id}::session::{session_id}");
    let key = format!("agent::{agent_id}");
    let event_channel = format!("chat-event-{session_id}");

    // Verify session exists
    {
        let sessions = state.interact_sessions.read().await;
        let fs = sessions
            .get(&key)
            .ok_or_else(|| "no sessions for this agent".to_string())?;
        fs.get_session(&session_id)
            .ok_or_else(|| "session not found".to_string())?;
    }

    // Replay buffered events
    let buffered_count = {
        let buffers = state.chat_event_buffers.lock().await;
        if let Some(events) = buffers.get(&proc_key) {
            for event_str in events {
                let payload = if let Some((etype, data)) = event_str.split_once(':') {
                    json!({ "type": etype, "data": data })
                } else {
                    json!({ "type": "message", "data": event_str })
                };
                let _ = app.emit(&event_channel, &payload);
            }
            events.len()
        } else {
            0
        }
    };

    // Re-subscribe if stream is active
    let is_active = {
        let streams = state.session_streams.lock().await;
        if let Some(tx) = streams.get(&proc_key) {
            let mut rx = tx.subscribe();
            let app_clone = app.clone();
            let ec = event_channel.clone();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(event_str) => {
                            let payload =
                                if let Some((etype, data)) = event_str.split_once(':') {
                                    json!({ "type": etype, "data": data })
                                } else {
                                    json!({ "type": "message", "data": &event_str })
                                };
                            let _ = app_clone.emit(&ec, &payload);
                            if event_str.starts_with("done:") {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            true
        } else {
            false
        }
    };

    Ok(json!({
        "session_id": session_id,
        "buffered_events_replayed": buffered_count,
        "live_stream_active": is_active,
    }))
}

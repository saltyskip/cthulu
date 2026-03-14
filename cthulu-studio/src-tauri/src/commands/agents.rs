use serde::Deserialize;
use serde_json::{json, Value};

use cthulu::agents::{AgentHooks, SubAgents, STUDIO_ASSISTANT_ID};
use cthulu::api::AppState;

// ---------------------------------------------------------------------------
// List agents
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_agents(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let agents = state.agent_repo.list().await;

    let summaries: Vec<Value> = agents
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "name": a.name,
                "description": a.description,
                "permissions": a.permissions,
                "hooks": a.hooks,
                "subagent_only": a.subagent_only,
                "subagent_count": a.subagents.len(),
                "reports_to": a.reports_to,
                "role": a.role,
                "created_at": a.created_at,
                "updated_at": a.updated_at,
                "project": a.project,
            })
        })
        .collect();

    Ok(json!({ "agents": summaries }))
}

// ---------------------------------------------------------------------------
// Get agent
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_agent(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let agent = state
        .agent_repo
        .get(&id)
        .await
        .ok_or_else(|| "agent not found".to_string())?;

    serde_json::to_value(&agent).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Create agent
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAgentRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    append_system_prompt: Option<String>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    hooks: AgentHooks,
    #[serde(default)]
    subagents: SubAgents,
    #[serde(default)]
    subagent_only: bool,
    #[serde(default)]
    reports_to: Option<String>,
    #[serde(default)]
    role: Option<String>,
}

#[tauri::command]
pub async fn create_agent(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: CreateAgentRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let mut builder = cthulu::agents::Agent::builder(uuid::Uuid::new_v4().to_string())
        .name(request.name)
        .description(request.description)
        .prompt(request.prompt)
        .permissions(request.permissions)
        .hooks(request.hooks)
        .subagents(request.subagents)
        .subagent_only(request.subagent_only);
    if let Some(s) = request.append_system_prompt {
        builder = builder.append_system_prompt(s);
    }
    if let Some(w) = request.working_dir {
        builder = builder.working_dir(w);
    }
    if let Some(r) = request.reports_to {
        builder = builder.reports_to(r);
    }
    if let Some(r) = request.role {
        builder = builder.role(r);
    }
    let agent = builder.build();

    let id = agent.id.clone();
    state
        .agent_repo
        .save(agent)
        .await
        .map_err(|e| format!("failed to save agent: {e}"))?;

    let _ = state.changes_tx.send(cthulu::api::changes::ResourceChangeEvent {
        resource_type: cthulu::api::changes::ResourceType::Agent,
        change_type: cthulu::api::changes::ChangeType::Created,
        resource_id: id.clone(),
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({ "id": id }))
}

// ---------------------------------------------------------------------------
// Update agent
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UpdateAgentRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    permissions: Option<Vec<String>>,
    #[serde(default)]
    append_system_prompt: Option<Option<String>>,
    #[serde(default)]
    working_dir: Option<Option<String>>,
    #[serde(default)]
    hooks: Option<AgentHooks>,
    #[serde(default)]
    subagents: Option<SubAgents>,
    #[serde(default)]
    subagent_only: Option<bool>,
    #[serde(default)]
    project: Option<Option<String>>,
    // Hierarchy fields
    #[serde(default)]
    reports_to: Option<Option<String>>,
    #[serde(default)]
    role: Option<Option<String>>,
    // Heartbeat config fields
    #[serde(default)]
    heartbeat_enabled: Option<bool>,
    #[serde(default)]
    heartbeat_interval_secs: Option<u64>,
    #[serde(default)]
    heartbeat_prompt_template: Option<String>,
    #[serde(default)]
    max_turns_per_heartbeat: Option<u32>,
    #[serde(default)]
    auto_permissions: Option<bool>,
}

#[tauri::command]
pub async fn update_agent(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
    request: UpdateAgentRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let mut agent = state
        .agent_repo
        .get(&id)
        .await
        .ok_or_else(|| "agent not found".to_string())?;

    if let Some(name) = request.name {
        agent.name = name;
    }
    if let Some(description) = request.description {
        agent.description = description;
    }
    if let Some(prompt) = request.prompt {
        agent.prompt = prompt;
    }
    if let Some(permissions) = request.permissions {
        agent.permissions = permissions;
    }
    if let Some(append_system_prompt) = request.append_system_prompt {
        agent.append_system_prompt = append_system_prompt;
    }
    if let Some(working_dir) = request.working_dir {
        agent.working_dir = working_dir;
    }
    if let Some(hooks) = request.hooks {
        agent.hooks = hooks;
    }
    if let Some(subagents) = request.subagents {
        agent.subagents = subagents;
    }
    if let Some(subagent_only) = request.subagent_only {
        agent.subagent_only = subagent_only;
    }
    if let Some(project) = request.project {
        agent.project = project;
    }
    // Hierarchy
    if let Some(reports_to) = request.reports_to {
        if let Some(ref target_id) = reports_to {
            if target_id == &id {
                return Err("agent cannot report to itself".to_string());
            }
            // Walk up the chain to detect cycles
            let mut current = target_id.clone();
            for _ in 0..100 {
                if let Some(parent) = state.agent_repo.get(&current).await {
                    match &parent.reports_to {
                        Some(parent_rt) if parent_rt == &id => {
                            return Err("circular reporting chain detected".to_string());
                        }
                        Some(parent_rt) => current = parent_rt.clone(),
                        None => break,
                    }
                } else {
                    break;
                }
            }
        }
        agent.reports_to = reports_to;
    }
    if let Some(role) = request.role {
        agent.role = role;
    }
    // Heartbeat config
    let heartbeat_changed = request.heartbeat_enabled.is_some()
        || request.heartbeat_interval_secs.is_some();
    if let Some(heartbeat_enabled) = request.heartbeat_enabled {
        agent.heartbeat_enabled = heartbeat_enabled;
    }
    if let Some(heartbeat_interval_secs) = request.heartbeat_interval_secs {
        agent.heartbeat_interval_secs = heartbeat_interval_secs;
    }
    if let Some(heartbeat_prompt_template) = request.heartbeat_prompt_template {
        agent.heartbeat_prompt_template = heartbeat_prompt_template;
    }
    if let Some(max_turns_per_heartbeat) = request.max_turns_per_heartbeat {
        agent.max_turns_per_heartbeat = max_turns_per_heartbeat;
    }
    if let Some(auto_permissions) = request.auto_permissions {
        agent.auto_permissions = auto_permissions;
    }
    agent.updated_at = chrono::Utc::now();

    state
        .agent_repo
        .save(agent.clone())
        .await
        .map_err(|e| format!("failed to save agent: {e}"))?;

    // Sync heartbeat scheduler if heartbeat config changed
    if heartbeat_changed {
        let scheduler = state.heartbeat_scheduler.read().await;
        scheduler.sync_agent(&id).await;
    }

    let _ = state.changes_tx.send(cthulu::api::changes::ResourceChangeEvent {
        resource_type: cthulu::api::changes::ResourceType::Agent,
        change_type: cthulu::api::changes::ChangeType::Updated,
        resource_id: id,
        timestamp: chrono::Utc::now(),
    });

    serde_json::to_value(&agent).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Delete agent
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn delete_agent(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    if id == STUDIO_ASSISTANT_ID {
        return Err("cannot delete the built-in Studio Assistant".to_string());
    }

    // Orphan any agents that reported to this one
    let all_agents = state.agent_repo.list().await;
    for mut subordinate in all_agents {
        if subordinate.reports_to.as_deref() == Some(&id) {
            subordinate.reports_to = None;
            subordinate.updated_at = chrono::Utc::now();
            let _ = state.agent_repo.save(subordinate).await;
        }
    }

    let existed = state
        .agent_repo
        .delete(&id)
        .await
        .map_err(|e| format!("failed to delete agent: {e}"))?;

    if !existed {
        return Err("agent not found".to_string());
    }

    // Stop heartbeat timer for deleted agent
    {
        let scheduler = state.heartbeat_scheduler.read().await;
        scheduler.sync_agent(&id).await;
    }

    let _ = state.changes_tx.send(cthulu::api::changes::ResourceChangeEvent {
        resource_type: cthulu::api::changes::ResourceType::Agent,
        change_type: cthulu::api::changes::ChangeType::Deleted,
        resource_id: id,
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({ "deleted": true }))
}

// ---------------------------------------------------------------------------
// List agent sessions
// ---------------------------------------------------------------------------

const MAX_INTERACTIVE_SESSIONS: usize = 5;

#[tauri::command]
pub async fn list_agent_sessions(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let key = format!("agent::{agent_id}");
    let sessions = state.interact_sessions.read().await;

    if let Some(flow_sessions) = sessions.get(&key) {
        let mut pool = state.live_processes.lock().await;
        let sdk_pool = state.sdk_sessions.lock().await;
        let interactive_count = flow_sessions
            .sessions
            .iter()
            .filter(|s| s.kind == "interactive")
            .count();

        let list: Vec<Value> = flow_sessions
            .sessions
            .iter()
            .map(|s| {
                let proc_k = format!("agent::{agent_id}::session::{}", s.session_id);
                let process_alive = if let Some(proc) = pool.get_mut(&proc_k) {
                    !matches!(proc.child.try_wait(), Ok(Some(_)))
                } else if let Some(sdk_session) = sdk_pool.get(&proc_k) {
                    sdk_session.is_connected()
                } else {
                    false
                };
                let mut v = json!({
                    "session_id": s.session_id,
                    "summary": s.summary,
                    "message_count": s.message_count,
                    "total_cost": s.total_cost,
                    "created_at": s.created_at,
                    "busy": s.busy,
                    "kind": s.kind,
                    "process_alive": process_alive,
                });
                if let Some(ref fr) = s.flow_run {
                    v["flow_run"] = serde_json::to_value(fr).unwrap_or_default();
                }
                v
            })
            .collect();

        Ok(json!({
            "agent_id": agent_id,
            "active_session": flow_sessions.active_session,
            "sessions": list,
            "interactive_count": interactive_count,
            "max_interactive_sessions": MAX_INTERACTIVE_SESSIONS,
        }))
    } else {
        Ok(json!({
            "agent_id": agent_id,
            "active_session": "",
            "sessions": [],
            "interactive_count": 0,
            "max_interactive_sessions": MAX_INTERACTIVE_SESSIONS,
        }))
    }
}

// ---------------------------------------------------------------------------
// New agent session
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn new_agent_session(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let agent = state
        .agent_repo
        .get(&agent_id)
        .await
        .ok_or_else(|| "agent not found".to_string())?;

    let original_working_dir = agent.working_dir.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| ".".into())
            .to_string_lossy()
            .to_string()
    });

    let key = format!("agent::{agent_id}");
    let new_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Try to create a worktree group for git isolation
    let (working_dir, worktree_group) = match cthulu::git::create_worktree_group(
        std::path::Path::new(&original_working_dir),
        &new_id,
    ) {
        Ok(group) => {
            let meta = cthulu::git::WorktreeGroupMeta::from(&group);
            let wt_working_dir = group.shadow_root.to_string_lossy().to_string();
            (wt_working_dir, Some(meta))
        }
        Err(_) => (original_working_dir, None),
    };

    let mut all_sessions = state.interact_sessions.write().await;
    let flow_sessions = all_sessions
        .entry(key)
        .or_insert_with(|| cthulu::api::FlowSessions {
            flow_name: agent.name.clone(),
            active_session: String::new(),
            sessions: Vec::new(),
        });

    // Enforce session limit
    let interactive_count = flow_sessions
        .sessions
        .iter()
        .filter(|s| s.kind == "interactive")
        .count();
    if interactive_count >= MAX_INTERACTIVE_SESSIONS {
        return Err(format!(
            "session limit reached ({MAX_INTERACTIVE_SESSIONS} interactive sessions max)"
        ));
    }

    flow_sessions.sessions.push(cthulu::api::InteractSession {
        session_id: new_id.clone(),
        summary: String::new(),
        node_id: None,
        working_dir,
        active_pid: None,
        busy: false,
        busy_since: None,
        message_count: 0,
        total_cost: 0.0,
        created_at: now.clone(),
        skills_dir: None,
        kind: "interactive".to_string(),
        flow_run: None,
        worktree_group,
    });
    flow_sessions.active_session = new_id.clone();

    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_sessions_to_disk(&sessions_snapshot);

    Ok(json!({ "session_id": new_id, "created_at": now }))
}

// ---------------------------------------------------------------------------
// Delete agent session
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn delete_agent_session(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let key = format!("agent::{agent_id}");
    let proc_k = format!("agent::{agent_id}::session::{session_id}");

    // Remove per-session live process
    {
        let mut pool = state.live_processes.lock().await;
        pool.remove(&proc_k);
    }

    // Disconnect SDK session if present
    {
        let mut sdk_pool = state.sdk_sessions.lock().await;
        if let Some(mut session) = sdk_pool.remove(&proc_k) {
            let _ = session.disconnect().await;
        }
    }

    let mut all_sessions = state.interact_sessions.write().await;

    let active_after = {
        let flow_sessions = all_sessions
            .get_mut(&key)
            .ok_or_else(|| "no sessions for this agent".to_string())?;

        if flow_sessions.sessions.len() <= 1 {
            return Err("cannot delete the last session".to_string());
        }

        // Clean up worktree group if present
        if let Some(session) = flow_sessions.get_session(&session_id) {
            if let Some(ref wt_meta) = session.worktree_group {
                let group = wt_meta.to_worktree_group();
                let _ = cthulu::git::remove_worktree_group(&group);
            }
        }

        flow_sessions
            .sessions
            .retain(|s| s.session_id != session_id);

        if flow_sessions.active_session == session_id {
            if let Some(last) = flow_sessions.sessions.last() {
                flow_sessions.active_session = last.session_id.clone();
            }
        }

        flow_sessions.active_session.clone()
    };

    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_sessions_to_disk(&sessions_snapshot);

    Ok(json!({
        "deleted": true,
        "active_session": active_after,
    }))
}

// ---------------------------------------------------------------------------
// Session status
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_session_status(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let key = format!("agent::{agent_id}");
    let proc_k = format!("agent::{agent_id}::session::{session_id}");

    let sessions = state.interact_sessions.read().await;
    let flow_sessions = sessions
        .get(&key)
        .ok_or_else(|| "no sessions for this agent".to_string())?;
    let session = flow_sessions
        .get_session(&session_id)
        .ok_or_else(|| "session not found".to_string())?;

    let mut pool = state.live_processes.lock().await;
    let sdk_pool = state.sdk_sessions.lock().await;
    let process_alive = if let Some(proc) = pool.get_mut(&proc_k) {
        !matches!(proc.child.try_wait(), Ok(Some(_)))
    } else if let Some(sdk_session) = sdk_pool.get(&proc_k) {
        sdk_session.is_connected()
    } else {
        false
    };

    Ok(json!({
        "session_id": session_id,
        "busy": session.busy,
        "busy_since": session.busy_since.map(|t| t.to_rfc3339()),
        "process_alive": process_alive,
        "message_count": session.message_count,
        "total_cost": session.total_cost,
    }))
}

// ---------------------------------------------------------------------------
// Kill session
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn kill_session(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let key = format!("agent::{agent_id}");
    let proc_k = format!("agent::{agent_id}::session::{session_id}");

    // Remove live process (Drop impl sends SIGKILL)
    {
        let mut pool = state.live_processes.lock().await;
        pool.remove(&proc_k);
    }

    // Disconnect SDK session if present
    {
        let mut sdk_pool = state.sdk_sessions.lock().await;
        if let Some(mut session) = sdk_pool.remove(&proc_k) {
            let _ = session.disconnect().await;
        }
    }

    // Clear busy flag
    let mut all_sessions = state.interact_sessions.write().await;
    if let Some(flow_sessions) = all_sessions.get_mut(&key) {
        if let Some(session) = flow_sessions.get_session_mut(&session_id) {
            session.active_pid = None;
            session.busy = false;
            session.busy_since = None;
        }
    }

    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_sessions_to_disk(&sessions_snapshot);

    Ok(json!({ "status": "killed" }))
}

// ---------------------------------------------------------------------------
// Get session log
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_session_log(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let _ = agent_id; // included for API consistency
    let logs_dir = state.data_dir.join("session_logs");
    let log_path = logs_dir.join(format!("{session_id}.jsonl"));

    if !log_path.exists() {
        return Ok(json!({ "lines": [] }));
    }

    let content = tokio::fs::read_to_string(&log_path)
        .await
        .map_err(|e| format!("failed to read log: {e}"))?;

    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
    Ok(json!({ "lines": lines }))
}

// ---------------------------------------------------------------------------
// List session files
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_session_files(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let key = format!("agent::{agent_id}");
    let all_sessions = state.interact_sessions.read().await;
    let working_dir = all_sessions
        .get(&key)
        .and_then(|fs| fs.get_session(&session_id))
        .map(|s| s.working_dir.clone());
    drop(all_sessions);

    let working_dir = working_dir.ok_or_else(|| "session not found".to_string())?;

    let dir = std::path::Path::new(&working_dir);
    if !dir.exists() || !dir.is_dir() {
        return Ok(json!({ "tree": [] }));
    }

    let tree = build_file_tree(dir, dir, 20);
    Ok(json!({ "tree": tree, "root": working_dir }))
}

// ---------------------------------------------------------------------------
// Read session file
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn read_session_file(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: String,
    path: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let key = format!("agent::{agent_id}");
    let all_sessions = state.interact_sessions.read().await;
    let working_dir = all_sessions
        .get(&key)
        .and_then(|fs| fs.get_session(&session_id))
        .map(|s| s.working_dir.clone());
    drop(all_sessions);

    let working_dir = working_dir.ok_or_else(|| "session not found".to_string())?;

    if path.is_empty() {
        return Err("path parameter is required".to_string());
    }

    let base = std::path::Path::new(&working_dir)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&working_dir));
    let target = base.join(&path);

    // Security: ensure the resolved path is within the working directory
    let resolved = target
        .canonicalize()
        .map_err(|_| "file not found".to_string())?;
    if !resolved.starts_with(&base) {
        return Err("path traversal not allowed".to_string());
    }

    if !resolved.is_file() {
        return Err("not a file".to_string());
    }

    // Limit file size to 1MB
    let metadata =
        std::fs::metadata(&resolved).map_err(|_| "cannot read file metadata".to_string())?;
    if metadata.len() > 1_048_576 {
        return Err("file too large (>1MB)".to_string());
    }

    let content = std::fs::read_to_string(&resolved)
        .map_err(|_| "cannot read file as text (may be binary)".to_string())?;

    Ok(json!({
        "path": path,
        "content": content,
        "size": metadata.len(),
    }))
}

// ---------------------------------------------------------------------------
// Git snapshot
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_git_snapshot(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let key = format!("agent::{agent_id}");
    let sessions = state.interact_sessions.read().await;
    let flow_sessions = sessions
        .get(&key)
        .ok_or_else(|| "no sessions for this agent".to_string())?;
    let session = flow_sessions
        .get_session(&session_id)
        .ok_or_else(|| "session not found".to_string())?;

    let wt_meta = session
        .worktree_group
        .as_ref()
        .ok_or_else(|| "no git integration for this session".to_string())?;

    let snapshot = cthulu::git::snapshot_from_meta(wt_meta);
    serde_json::to_value(&snapshot).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Git diff
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_git_diff(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    session_id: String,
    path: String,
    repo_root: Option<String>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let key = format!("agent::{agent_id}");
    let sessions = state.interact_sessions.read().await;
    let flow_sessions = sessions
        .get(&key)
        .ok_or_else(|| "no sessions for this agent".to_string())?;
    let session = flow_sessions
        .get_session(&session_id)
        .ok_or_else(|| "session not found".to_string())?;

    let wt_meta = session
        .worktree_group
        .as_ref()
        .ok_or_else(|| "no git integration for this session".to_string())?;

    // Find the correct repo entry
    let entry = if wt_meta.single_repo {
        wt_meta.repos.first()
    } else {
        let target_root = repo_root.as_deref().unwrap_or(".");
        wt_meta
            .repos
            .iter()
            .find(|r| {
                let wt = std::path::Path::new(&r.worktree_path);
                let shadow = std::path::Path::new(&wt_meta.shadow_root);
                if let Ok(rel) = wt.strip_prefix(shadow) {
                    rel.to_string_lossy() == target_root
                } else {
                    false
                }
            })
            .or_else(|| wt_meta.repos.first())
    };

    let entry = entry.ok_or_else(|| "no repos in worktree group".to_string())?;

    let worktree_path = std::path::Path::new(&entry.worktree_path);
    let diff = cthulu::git::diff_file(worktree_path, &path);

    Ok(json!({
        "diff": diff.unwrap_or_default(),
        "path": path,
        "repo_root": repo_root.unwrap_or_else(|| ".".to_string()),
    }))
}

// ---------------------------------------------------------------------------
// File tree helper
// ---------------------------------------------------------------------------

fn build_file_tree(
    dir: &std::path::Path,
    root: &std::path::Path,
    max_depth: u32,
) -> Vec<Value> {
    if max_depth == 0 {
        return vec![];
    }

    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };

    let mut items: Vec<_> = read_dir.flatten().collect();
    items.sort_by_key(|e| e.file_name());

    for entry in items {
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files/dirs and common noise
        if name.starts_with('.')
            || name == "node_modules"
            || name == "target"
            || name == "__pycache__"
        {
            continue;
        }

        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        if path.is_dir() {
            let children = build_file_tree(&path, root, max_depth - 1);
            entries.push(json!({
                "name": name,
                "path": rel,
                "type": "directory",
                "children": children,
            }));
        } else {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            entries.push(json!({
                "name": name,
                "path": rel,
                "type": "file",
                "size": size,
            }));
        }
    }

    entries
}

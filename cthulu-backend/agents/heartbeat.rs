use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::agents::repository::AgentRepository;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeartbeatRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

/// How a heartbeat run was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeupSource {
    /// Scheduled timer tick.
    Timer,
    /// Manual trigger (button / API).
    OnDemand,
    /// Task assignment triggered wakeup (Phase 2).
    Assignment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRun {
    pub id: String,
    pub agent_id: String,
    pub status: HeartbeatRunStatus,
    pub source: WakeupSource,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    pub cost_usd: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::claude_adapter::parse::UsageSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub log_path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Duration in seconds.
    pub duration_secs: f64,
}

const MAX_RUNS_PER_AGENT: usize = 100;

// ---------------------------------------------------------------------------
// File persistence helpers
// ---------------------------------------------------------------------------

/// Directory for run metadata: ~/.cthulu/heartbeat_runs/{agent_id}/
fn runs_dir(data_dir: &std::path::Path, agent_id: &str) -> PathBuf {
    data_dir.join("heartbeat_runs").join(agent_id)
}

/// Path to the runs.json index: ~/.cthulu/heartbeat_runs/{agent_id}/runs.json
fn runs_json_path(data_dir: &std::path::Path, agent_id: &str) -> PathBuf {
    runs_dir(data_dir, agent_id).join("runs.json")
}

/// Path to the active run lock: ~/.cthulu/heartbeat_runs/{agent_id}/active_run.json
fn active_run_path(data_dir: &std::path::Path, agent_id: &str) -> PathBuf {
    runs_dir(data_dir, agent_id).join("active_run.json")
}

/// Load persisted runs for an agent from disk.
fn load_runs_from_disk(data_dir: &std::path::Path, agent_id: &str) -> Vec<HeartbeatRun> {
    let path = runs_json_path(data_dir, agent_id);
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Persist runs for an agent to disk (atomic: write tmp + rename).
fn save_runs_to_disk(data_dir: &std::path::Path, agent_id: &str, runs: &[HeartbeatRun]) {
    let dir = runs_dir(data_dir, agent_id);
    let _ = std::fs::create_dir_all(&dir);
    let path = runs_json_path(data_dir, agent_id);
    let tmp = path.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_string_pretty(runs) {
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// Write the active run ID to lock file.
fn write_active_run(data_dir: &std::path::Path, agent_id: &str, run_id: &str) {
    let dir = runs_dir(data_dir, agent_id);
    let _ = std::fs::create_dir_all(&dir);
    let path = active_run_path(data_dir, agent_id);
    let _ = std::fs::write(&path, run_id);
}

/// Clear the active run lock file.
fn clear_active_run(data_dir: &std::path::Path, agent_id: &str) {
    let path = active_run_path(data_dir, agent_id);
    let _ = std::fs::remove_file(&path);
}

/// Check if there is an active run for this agent.
fn read_active_run(data_dir: &std::path::Path, agent_id: &str) -> Option<String> {
    let path = active_run_path(data_dir, agent_id);
    std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
}

/// Get the last successful session_id for an agent (for --resume).
fn last_session_id(runs: &[HeartbeatRun]) -> Option<String> {
    runs.iter()
        .find(|r| r.status == HeartbeatRunStatus::Succeeded)
        .and_then(|r| r.session_id.clone())
}

// ---------------------------------------------------------------------------
// HeartbeatScheduler
// ---------------------------------------------------------------------------

pub struct HeartbeatScheduler {
    agent_repo: Arc<dyn AgentRepository>,
    data_dir: PathBuf,
    /// Recent runs per agent (agent_id -> Vec<HeartbeatRun>), newest first.
    runs: Arc<RwLock<HashMap<String, Vec<HeartbeatRun>>>>,
    /// Tokio task handles for each scheduled agent.
    handles: Mutex<HashMap<String, JoinHandle<()>>>,
    /// Shutdown signal — set to true when the server is stopping.
    shutdown: Arc<tokio::sync::watch::Sender<bool>>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl HeartbeatScheduler {
    pub fn new(agent_repo: Arc<dyn AgentRepository>, data_dir: PathBuf) -> Self {
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
        Self {
            agent_repo,
            data_dir,
            runs: Arc::new(RwLock::new(HashMap::new())),
            handles: Mutex::new(HashMap::new()),
            shutdown: Arc::new(shutdown),
            shutdown_rx,
        }
    }

    /// Start heartbeat timers for all agents with `heartbeat_enabled`.
    /// Also loads persisted run history from disk and reaps orphaned active runs.
    pub async fn start_all(&self) {
        // Load persisted runs from disk for all agents
        self.load_all_persisted_runs().await;

        // Reap orphaned active runs (from previous crash/restart)
        self.reap_orphaned_runs().await;

        let agents = self.agent_repo.list().await;
        for agent in agents {
            if agent.heartbeat_enabled {
                self.start_agent_timer(&agent.id, agent.heartbeat_interval_secs)
                    .await;
            }
        }
    }

    /// Stop all running heartbeat timers. Mark any in-progress runs as cancelled.
    pub async fn stop_all(&self) {
        let _ = self.shutdown.send(true);

        // Mark active runs as cancelled
        {
            let mut all_runs = self.runs.write().await;
            for (agent_id, agent_runs) in all_runs.iter_mut() {
                for run in agent_runs.iter_mut() {
                    if run.status == HeartbeatRunStatus::Running
                        || run.status == HeartbeatRunStatus::Queued
                    {
                        run.status = HeartbeatRunStatus::Cancelled;
                        run.finished_at = Some(Utc::now());
                        run.error = Some("Server shutdown".into());
                        if run.finished_at.is_some() && run.started_at < Utc::now() {
                            run.duration_secs = (Utc::now() - run.started_at)
                                .num_milliseconds() as f64
                                / 1000.0;
                        }
                    }
                }
                save_runs_to_disk(&self.data_dir, agent_id, agent_runs);
                clear_active_run(&self.data_dir, agent_id);
            }
        }

        let mut handles = self.handles.lock().await;
        for (_, handle) in handles.drain() {
            handle.abort();
        }
    }

    /// Reconfigure a single agent's timer (called after agent create/update/delete).
    pub async fn sync_agent(&self, agent_id: &str) {
        // Cancel existing timer if any
        {
            let mut handles = self.handles.lock().await;
            if let Some(handle) = handles.remove(agent_id) {
                handle.abort();
            }
        }
        // Start new timer if agent exists and heartbeat is enabled
        if let Some(agent) = self.agent_repo.get(agent_id).await {
            if agent.heartbeat_enabled {
                self.start_agent_timer(&agent.id, agent.heartbeat_interval_secs)
                    .await;
            }
        }
    }

    /// Manually trigger a heartbeat run for an agent (POST /agents/{id}/wakeup).
    pub async fn wakeup(&self, agent_id: &str) -> Result<HeartbeatRun, String> {
        let agent = self
            .agent_repo
            .get(agent_id)
            .await
            .ok_or_else(|| format!("agent not found: {agent_id}"))?;

        // Check concurrent run guard
        if let Some(active_id) = read_active_run(&self.data_dir, agent_id) {
            return Err(format!(
                "agent already has an active run: {active_id}. Wait for it to complete."
            ));
        }

        self.execute_heartbeat(&agent, WakeupSource::OnDemand).await
    }

    /// Trigger a heartbeat with a specific source and optional task context appended to the prompt.
    pub async fn wakeup_with_source(
        &self,
        agent_id: &str,
        source: WakeupSource,
        task_context: Option<&str>,
    ) -> Result<HeartbeatRun, String> {
        let mut agent = self
            .agent_repo
            .get(agent_id)
            .await
            .ok_or_else(|| format!("agent not found: {agent_id}"))?;

        // Check concurrent run guard
        if let Some(active_id) = read_active_run(&self.data_dir, agent_id) {
            return Err(format!(
                "agent already has an active run: {active_id}. Wait for it to complete."
            ));
        }

        // If task context provided, append it to the heartbeat prompt for this run
        if let Some(ctx) = task_context {
            agent.heartbeat_prompt_template = format!(
                "{}\n\n## New Assignment\n{}",
                agent.heartbeat_prompt_template, ctx
            );
        }

        self.execute_heartbeat(&agent, source).await
    }

    /// Get all runs for an agent (newest first).
    pub async fn runs_for(&self, agent_id: &str) -> Vec<HeartbeatRun> {
        let runs = self.runs.read().await;
        runs.get(agent_id).cloned().unwrap_or_default()
    }

    /// Get a specific run by ID.
    pub async fn get_run(&self, run_id: &str) -> Option<HeartbeatRun> {
        let runs = self.runs.read().await;
        for agent_runs in runs.values() {
            if let Some(run) = agent_runs.iter().find(|r| r.id == run_id) {
                return Some(run.clone());
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Load persisted run history from disk for all agent directories.
    async fn load_all_persisted_runs(&self) {
        let runs_base = self.data_dir.join("heartbeat_runs");
        if !runs_base.exists() {
            return;
        }
        let entries = match std::fs::read_dir(&runs_base) {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut all_runs = self.runs.write().await;
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let agent_id = entry.file_name().to_string_lossy().to_string();
                let runs = load_runs_from_disk(&self.data_dir, &agent_id);
                if !runs.is_empty() {
                    tracing::info!(
                        agent_id = %agent_id,
                        count = runs.len(),
                        "heartbeat: loaded persisted runs"
                    );
                    all_runs.insert(agent_id, runs);
                }
            }
        }
    }

    /// Detect and mark orphaned active runs (from previous crash/restart).
    async fn reap_orphaned_runs(&self) {
        let runs_base = self.data_dir.join("heartbeat_runs");
        if !runs_base.exists() {
            return;
        }
        let entries = match std::fs::read_dir(&runs_base) {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut all_runs = self.runs.write().await;
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let agent_id = entry.file_name().to_string_lossy().to_string();
            if let Some(orphan_run_id) = read_active_run(&self.data_dir, &agent_id) {
                tracing::warn!(
                    agent_id = %agent_id,
                    run_id = %orphan_run_id,
                    "heartbeat: reaping orphaned active run"
                );
                // Mark the orphaned run as failed in the runs list
                if let Some(agent_runs) = all_runs.get_mut(&agent_id) {
                    if let Some(run) = agent_runs.iter_mut().find(|r| r.id == orphan_run_id) {
                        run.status = HeartbeatRunStatus::Failed;
                        run.finished_at = Some(Utc::now());
                        run.error = Some("Orphaned run (server restarted)".into());
                    }
                    save_runs_to_disk(&self.data_dir, &agent_id, agent_runs);
                }
                clear_active_run(&self.data_dir, &agent_id);
            }
        }
    }

    async fn start_agent_timer(&self, agent_id: &str, interval_secs: u64) {
        let agent_id_owned = agent_id.to_string();
        let agent_repo = self.agent_repo.clone();
        let runs = self.runs.clone();
        let data_dir = self.data_dir.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        let interval = std::time::Duration::from_secs(interval_secs.max(30)); // minimum 30s

        let handle = tokio::spawn(async move {
            // Initial delay — don't fire immediately on startup
            tokio::time::sleep(interval).await;

            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        // Check agent still exists and is enabled
                        let agent = match agent_repo.get(&agent_id_owned).await {
                            Some(a) if a.heartbeat_enabled => a,
                            _ => {
                                tracing::info!(
                                    agent_id = %agent_id_owned,
                                    "heartbeat: agent disabled or removed, stopping timer"
                                );
                                break;
                            }
                        };

                        // Concurrent run guard: skip if another run is active
                        if read_active_run(&data_dir, &agent_id_owned).is_some() {
                            tracing::info!(
                                agent_id = %agent_id_owned,
                                "heartbeat: skipping tick — previous run still active"
                            );
                            continue;
                        }

                        // Get last session_id for resume
                        let prev_session_id = {
                            let all_runs = runs.read().await;
                            all_runs.get(&agent_id_owned)
                                .and_then(|r| last_session_id(r))
                        };

                        let run = execute_heartbeat_run(
                            &agent,
                            &data_dir,
                            WakeupSource::Timer,
                            prev_session_id.as_deref(),
                        ).await;

                        let mut all_runs = runs.write().await;
                        let agent_runs = all_runs.entry(agent_id_owned.clone()).or_default();
                        agent_runs.insert(0, run); // newest first
                        if agent_runs.len() > MAX_RUNS_PER_AGENT {
                            agent_runs.truncate(MAX_RUNS_PER_AGENT);
                        }
                        // Persist to disk
                        save_runs_to_disk(&data_dir, &agent_id_owned, agent_runs);
                    }
                    _ = shutdown_rx.changed() => {
                        tracing::info!(
                            agent_id = %agent_id_owned,
                            "heartbeat: shutdown signal received"
                        );
                        break;
                    }
                }
            }
        });

        let mut handles = self.handles.lock().await;
        handles.insert(agent_id.to_string(), handle);
    }

    async fn execute_heartbeat(
        &self,
        agent: &crate::agents::Agent,
        source: WakeupSource,
    ) -> Result<HeartbeatRun, String> {
        // Get last session_id for resume
        let prev_session_id = {
            let all_runs = self.runs.read().await;
            all_runs.get(&agent.id).and_then(|r| last_session_id(r))
        };

        let run = execute_heartbeat_run(
            agent,
            &self.data_dir,
            source,
            prev_session_id.as_deref(),
        )
        .await;

        let mut all_runs = self.runs.write().await;
        let agent_runs = all_runs.entry(agent.id.clone()).or_default();
        agent_runs.insert(0, run.clone());
        if agent_runs.len() > MAX_RUNS_PER_AGENT {
            agent_runs.truncate(MAX_RUNS_PER_AGENT);
        }
        // Persist to disk
        save_runs_to_disk(&self.data_dir, &agent.id, agent_runs);
        Ok(run)
    }
}

// ---------------------------------------------------------------------------
// Heartbeat execution
// ---------------------------------------------------------------------------

/// Execute a single heartbeat run for an agent.
///
/// Spawns `claude --print - --output-format stream-json --verbose --max-turns N`
/// with the agent's heartbeat prompt, captures output, and parses the result.
///
/// Improvements over previous version:
/// - Concurrent run guard via active_run.json lock file
/// - Session continuity via --resume flag
/// - Subagent support via --agents flag
/// - --append-system-prompt support
/// - Wakeup source tracking
async fn execute_heartbeat_run(
    agent: &crate::agents::Agent,
    data_dir: &std::path::Path,
    source: WakeupSource,
    resume_session_id: Option<&str>,
) -> HeartbeatRun {
    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = Utc::now();
    let log_dir = data_dir.join("heartbeat_logs");
    let _ = tokio::fs::create_dir_all(&log_dir).await;
    let log_path = log_dir.join(format!("{run_id}.jsonl"));

    // Write active run lock
    write_active_run(data_dir, &agent.id, &run_id);

    // Build CLI args
    let mut args: Vec<String> = vec![
        "--print".into(),
        "-".into(), // read prompt from stdin
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
    ];

    if agent.max_turns_per_heartbeat > 0 {
        args.push("--max-turns".into());
        args.push(agent.max_turns_per_heartbeat.to_string());
    }

    if agent.auto_permissions {
        args.push("--dangerously-skip-permissions".into());
    } else if !agent.permissions.is_empty() {
        args.push("--allowedTools".into());
        args.push(agent.permissions.join(","));
    }

    // Session continuity: resume previous session if available
    if let Some(sid) = resume_session_id {
        args.push("--resume".into());
        args.push(sid.to_string());
    }

    // Append system prompt if configured
    if let Some(ref sys_prompt) = agent.append_system_prompt {
        if !sys_prompt.is_empty() {
            args.push("--append-system-prompt".into());
            args.push(sys_prompt.clone());
        }
    }

    // Pass sub-agent definitions via Claude Code's native --agents flag
    if !agent.subagents.is_empty() {
        if let Ok(agents_json) = serde_json::to_string(&agent.subagents) {
            args.push("--agents".into());
            args.push(agents_json);
            tracing::info!(
                agent_id = %agent.id,
                subagent_count = agent.subagents.len(),
                "heartbeat: passing sub-agents to claude CLI"
            );
        }
    }

    // Resolve working directory
    let working_dir = agent
        .working_dir
        .clone()
        .unwrap_or_else(|| ".".into());

    // Resolve prompt
    let prompt = if agent.heartbeat_prompt_template.is_empty() {
        "Continue your work. Check for pending tasks and complete any in-progress items."
            .to_string()
    } else {
        agent.heartbeat_prompt_template.clone()
    };

    tracing::info!(
        agent_id = %agent.id,
        agent_name = %agent.name,
        run_id = %run_id,
        source = ?source,
        resume_session = resume_session_id.is_some(),
        "heartbeat: starting run"
    );

    // Spawn claude CLI process
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let spawn_result = Command::new("claude")
        .args(&args)
        .current_dir(&working_dir)
        .env("CLAUDECODE", "")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match spawn_result {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(
                agent_id = %agent.id,
                error = %e,
                "heartbeat: failed to spawn claude"
            );
            clear_active_run(data_dir, &agent.id);
            return HeartbeatRun {
                id: run_id,
                agent_id: agent.id.clone(),
                status: HeartbeatRunStatus::Failed,
                source,
                started_at,
                finished_at: Some(Utc::now()),
                cost_usd: 0.0,
                usage: None,
                error: Some(format!("Failed to spawn claude: {e}")),
                log_path,
                model: None,
                session_id: None,
                duration_secs: 0.0,
            };
        }
    };

    // Write prompt to stdin and close it so claude reads EOF
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes()).await;
        let _ = stdin.write_all(b"\n").await;
        drop(stdin);
    }

    // Take stdout/stderr handles before waiting so we can read them independently.
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();

    // Spawn readers for stdout/stderr
    let stdout_handle = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = String::new();
        if let Some(mut out) = child_stdout {
            let _ = out.read_to_string(&mut buf).await;
        }
        buf
    });
    let stderr_handle = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = String::new();
        if let Some(mut err) = child_stderr {
            let _ = err.read_to_string(&mut buf).await;
        }
        buf
    });

    // Wait with timeout (heartbeat_interval as timeout, minimum 5 min)
    let timeout_secs = agent.heartbeat_interval_secs.max(300);
    let timeout_duration = std::time::Duration::from_secs(timeout_secs);

    let exit_status = match tokio::time::timeout(timeout_duration, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            tracing::error!(
                agent_id = %agent.id,
                error = %e,
                "heartbeat: process error"
            );
            let finished_at = Utc::now();
            clear_active_run(data_dir, &agent.id);
            return HeartbeatRun {
                id: run_id,
                agent_id: agent.id.clone(),
                status: HeartbeatRunStatus::Failed,
                source,
                started_at,
                finished_at: Some(finished_at),
                cost_usd: 0.0,
                usage: None,
                error: Some(format!("Process error: {e}")),
                log_path,
                model: None,
                session_id: None,
                duration_secs: (finished_at - started_at).num_milliseconds() as f64 / 1000.0,
            };
        }
        Err(_) => {
            // Timeout — kill the process
            let _ = child.kill().await;
            let finished_at = Utc::now();
            tracing::warn!(
                agent_id = %agent.id,
                timeout_secs = timeout_secs,
                "heartbeat: run timed out"
            );
            clear_active_run(data_dir, &agent.id);
            return HeartbeatRun {
                id: run_id,
                agent_id: agent.id.clone(),
                status: HeartbeatRunStatus::TimedOut,
                source,
                started_at,
                finished_at: Some(finished_at),
                cost_usd: 0.0,
                usage: None,
                error: Some(format!("Timed out after {timeout_secs}s")),
                log_path,
                model: None,
                session_id: None,
                duration_secs: (finished_at - started_at).num_milliseconds() as f64 / 1000.0,
            };
        }
    };

    // Collect stdout/stderr from reader tasks
    let stdout = stdout_handle.await.unwrap_or_default();
    let stderr = stderr_handle.await.unwrap_or_default();

    let finished_at = Utc::now();
    let duration_secs = (finished_at - started_at).num_milliseconds() as f64 / 1000.0;

    // Write raw stdout to log file
    let _ = tokio::fs::write(&log_path, &stdout).await;

    // Parse the stream-json output
    let parsed = crate::claude_adapter::parse::parse_stream_json(&stdout);
    let login = crate::claude_adapter::parse::detect_login_required(&stdout, &stderr);

    let status = if login.requires_login {
        HeartbeatRunStatus::Failed
    } else if exit_status.success() {
        HeartbeatRunStatus::Succeeded
    } else {
        HeartbeatRunStatus::Failed
    };

    let error = if login.requires_login {
        Some("Claude CLI requires login. Run `claude login` in your terminal.".into())
    } else if !exit_status.success() {
        parsed
            .result_json
            .as_ref()
            .and_then(|v| crate::claude_adapter::parse::describe_failure(v))
            .or_else(|| {
                Some(format!(
                    "Claude exited with code {}",
                    exit_status.code().unwrap_or(-1)
                ))
            })
    } else {
        None
    };

    // Clear active run lock
    clear_active_run(data_dir, &agent.id);

    tracing::info!(
        agent_id = %agent.id,
        run_id = %run_id,
        status = ?status,
        source = ?source,
        cost = parsed.cost_usd.unwrap_or(0.0),
        duration_secs = duration_secs,
        "heartbeat: run completed"
    );

    HeartbeatRun {
        id: run_id,
        agent_id: agent.id.clone(),
        status,
        source,
        started_at,
        finished_at: Some(finished_at),
        cost_usd: parsed.cost_usd.unwrap_or(0.0),
        usage: parsed.usage,
        error,
        log_path,
        model: if parsed.model.is_empty() {
            None
        } else {
            Some(parsed.model)
        },
        session_id: parsed.session_id,
        duration_secs,
    }
}

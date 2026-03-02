pub mod agents;
pub mod auth;
pub mod changes;
pub mod flows;
pub mod middleware;
pub mod prompts;
mod routes;
pub mod sandbox;
pub mod scheduler;
pub mod templates;

use axum::Router;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::agents::repository::AgentRepository;
use crate::api::changes::ResourceChangeEvent;
use crate::flows::events::RunEvent;
use crate::flows::repository::FlowRepository;
use crate::flows::scheduler::FlowScheduler;
use crate::flows::session_bridge::FlowRunMeta;
use crate::github::client::GithubClient;
use crate::prompts::repository::PromptRepository;
use crate::sandbox::backends::vm_manager::VmManagerProvider;
use crate::sandbox::provider::SandboxProvider;

/// A single Claude Code session (one tab in the History list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractSession {
    /// The Claude session ID (UUID). Used with `--session-id` on first message,
    /// `--resume` on subsequent messages.
    pub session_id: String,
    /// Short summary of what this session is about (first ~80 chars of first prompt).
    #[serde(default)]
    pub summary: String,
    /// If set, this session belongs to a specific node (node-level chat).
    /// When None, it's a flow-level session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Mounted path — working directory for the claude process.
    pub working_dir: String,
    /// PID of the currently running claude process (if any).
    #[serde(skip)]
    pub active_pid: Option<u32>,
    /// Whether a message is currently being processed.
    #[serde(skip)]
    pub busy: bool,
    /// When the session became busy (for stale detection). None when idle.
    #[serde(skip)]
    pub busy_since: Option<chrono::DateTime<chrono::Utc>>,
    /// Number of messages exchanged in this session.
    pub message_count: u64,
    /// Cumulative cost (parsed from claude result events).
    pub total_cost: f64,
    /// When this session was created (ISO 8601).
    pub created_at: String,
    /// Path to the .skills/ directory for this session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills_dir: Option<String>,
    /// Session kind: "interactive" (default) or "flow_run".
    #[serde(default = "default_interactive")]
    pub kind: String,
    /// Flow run metadata — only present for flow_run sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_run: Option<FlowRunMeta>,
}

fn default_interactive() -> String {
    "interactive".to_string()
}

/// All sessions for a single workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowSessions {
    pub flow_name: String,
    /// The session_id that was last used (default tab when opening History).
    pub active_session: String,
    pub sessions: Vec<InteractSession>,
}

impl FlowSessions {
    pub fn get_session(&self, session_id: &str) -> Option<&InteractSession> {
        self.sessions.iter().find(|s| s.session_id == session_id)
    }

    pub fn get_session_mut(&mut self, session_id: &str) -> Option<&mut InteractSession> {
        self.sessions.iter_mut().find(|s| s.session_id == session_id)
    }

    /// Get the active session (the one referenced by `active_session`).
    #[allow(dead_code)]
    pub fn active(&self) -> Option<&InteractSession> {
        self.get_session(&self.active_session)
    }

    #[allow(dead_code)]
    pub fn active_mut(&mut self) -> Option<&mut InteractSession> {
        let id = self.active_session.clone();
        self.get_session_mut(&id)
    }
}

/// Minimal VM-to-node mapping persisted in `sessions.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmMapping {
    pub vm_id: u32,
    pub vm_name: String,         // e.g. "Executor-E01_a3f2b1"
    #[serde(default)]
    pub web_terminal_url: String, // e.g. "http://34.100.130.60:7700"
}

/// Root structure for `sessions.yaml`.
#[derive(Debug, Serialize, Deserialize)]
struct SessionsFile {
    sessions: HashMap<String, FlowSessions>,
    /// VM mappings keyed by "flow_id::node_id".
    #[serde(default)]
    vms: HashMap<String, VmMapping>,
}

/// Old format (pre-migration): single session per flow.
#[derive(Debug, Deserialize)]
struct LegacySessionsFile {
    sessions: HashMap<String, LegacyInteractSession>,
}

#[derive(Debug, Deserialize)]
struct LegacyInteractSession {
    session_id: String,
    #[serde(default)]
    flow_name: String,
    #[serde(default)]
    working_dir: String,
    message_count: u64,
    total_cost: f64,
}

/// Loaded data from `sessions.yaml` — sessions + VM mappings.
pub struct LoadedSessions {
    pub sessions: HashMap<String, FlowSessions>,
    pub vms: HashMap<String, VmMapping>,
}

/// Load persisted sessions from a YAML file.
/// Supports auto-migration from the old single-session-per-flow format.
/// Returns empty maps if the file doesn't exist or can't be parsed.
pub fn load_sessions(path: &Path) -> LoadedSessions {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %path.display(), error = %e, "failed to read sessions file");
            }
            return LoadedSessions {
                sessions: HashMap::new(),
                vms: HashMap::new(),
            };
        }
    };

    // Try new format first
    if let Ok(file) = serde_yaml::from_str::<SessionsFile>(&contents) {
        tracing::info!(
            sessions = file.sessions.len(),
            vms = file.vms.len(),
            "loaded persisted sessions"
        );
        return LoadedSessions {
            sessions: file.sessions,
            vms: file.vms,
        };
    }

    // Try legacy format (single session per flow) and auto-migrate
    if let Ok(legacy) = serde_yaml::from_str::<LegacySessionsFile>(&contents) {
        tracing::info!(count = legacy.sessions.len(), "migrating legacy sessions format");
        let migrated: HashMap<String, FlowSessions> = legacy
            .sessions
            .into_iter()
            .map(|(flow_id, old)| {
                    let session = InteractSession {
                    session_id: old.session_id.clone(),
                    summary: String::new(),
                    node_id: None,
                    working_dir: if old.working_dir.is_empty() {
                        ".".to_string()
                    } else {
                        old.working_dir
                    },
                    active_pid: None,
                    busy: false,
                    busy_since: None,
                    message_count: old.message_count,
                    total_cost: old.total_cost,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    skills_dir: None,
                    kind: default_interactive(),
                    flow_run: None,
                };
                let flow_sessions = FlowSessions {
                    flow_name: if old.flow_name.is_empty() {
                        "Unknown".to_string()
                    } else {
                        old.flow_name
                    },
                    active_session: old.session_id,
                    sessions: vec![session],
                };
                (flow_id, flow_sessions)
            })
            .collect();
        return LoadedSessions {
            sessions: migrated,
            vms: HashMap::new(),
        };
    }

    tracing::warn!(path = %path.display(), "failed to parse sessions file in any known format");
    LoadedSessions {
        sessions: HashMap::new(),
        vms: HashMap::new(),
    }
}

/// Persist sessions to a YAML file (atomic write via temp + rename).
pub fn save_sessions(
    path: &Path,
    sessions: &HashMap<String, FlowSessions>,
    vms: &HashMap<String, VmMapping>,
) {
    let file = SessionsFile {
        sessions: sessions
            .iter()
            .map(|(k, v)| {
                let clean_sessions: Vec<InteractSession> = v
                    .sessions
                    .iter()
                    .map(|s| InteractSession {
                        session_id: s.session_id.clone(),
                        summary: s.summary.clone(),
                        node_id: s.node_id.clone(),
                        working_dir: s.working_dir.clone(),
                        active_pid: None,
                        busy: false,
                        busy_since: None,
                        message_count: s.message_count,
                        total_cost: s.total_cost,
                        created_at: s.created_at.clone(),
                        skills_dir: s.skills_dir.clone(),
                        kind: s.kind.clone(),
                        flow_run: s.flow_run.clone(),
                    })
                    .collect();
                (
                    k.clone(),
                    FlowSessions {
                        flow_name: v.flow_name.clone(),
                        active_session: v.active_session.clone(),
                        sessions: clean_sessions,
                    },
                )
            })
            .collect(),
        vms: vms.clone(),
    };

    let yaml = match serde_yaml::to_string(&file) {
        Ok(y) => y,
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize sessions to YAML");
            return;
        }
    };

    let tmp_path = path.with_extension("yaml.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &yaml) {
        tracing::error!(path = %tmp_path.display(), error = %e, "failed to write sessions temp file");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        tracing::error!(error = %e, "failed to rename sessions temp file");
    }
}

/// A PTY-based Claude Code process for interactive terminal sessions.
/// Spawned via `portable_pty` so Claude runs in a real TTY with full TUI output.
pub struct PtyProcess {
    /// The master side of the PTY (read/write bytes).
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    /// The child process handle (for kill/wait).
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    /// The Claude session ID this PTY is running.
    pub session_id: String,
    /// PTY writer, taken once at spawn time and shared across reconnects.
    /// Wrapped in Arc<std::sync::Mutex> so multiple WS connections can write.
    pub writer: std::sync::Arc<std::sync::Mutex<Box<dyn std::io::Write + Send>>>,
    /// Broadcast channel for PTY output — a single persistent reader task writes here,
    /// and each WS connection subscribes. Avoids duplicate readers on reconnect.
    pub output_tx: broadcast::Sender<Vec<u8>>,
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        if let Err(e) = self.child.kill() {
            // ESRCH (no such process) is expected if the child already exited
            tracing::trace!(session_id = %self.session_id, error = %e, "PTY child kill on drop");
        } else {
            tracing::info!(session_id = %self.session_id, "killed PTY child process on drop");
        }
    }
}

/// A persistent Claude CLI process kept alive between messages.
/// Uses `--input-format stream-json` so we can write multiple prompts to stdin.
pub struct LiveClaudeProcess {
    /// Writer end of the child's stdin.
    pub stdin: tokio::process::ChildStdin,
    /// Reader that yields stdout lines.
    pub stdout_lines: tokio::sync::mpsc::UnboundedReceiver<String>,
    /// Reader that yields stderr lines.
    pub stderr_lines: tokio::sync::mpsc::UnboundedReceiver<String>,
    /// The child process handle (for kill).
    pub child: tokio::process::Child,
    /// Whether the process is currently processing a message.
    pub busy: bool,
}

impl Drop for LiveClaudeProcess {
    fn drop(&mut self) {
        if let Err(e) = self.child.start_kill() {
            tracing::trace!(error = %e, "LiveClaudeProcess kill on drop");
        } else {
            tracing::info!("killed LiveClaudeProcess on drop");
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub github_client: Option<Arc<dyn GithubClient>>,
    pub http_client: Arc<reqwest::Client>,
    pub flow_repo: Arc<dyn FlowRepository>,
    pub prompt_repo: Arc<dyn PromptRepository>,
    pub agent_repo: Arc<dyn AgentRepository>,
    pub scheduler: Arc<FlowScheduler>,
    pub events_tx: broadcast::Sender<RunEvent>,
    pub changes_tx: broadcast::Sender<ResourceChangeEvent>,
    /// Per-workflow session lists (flow_id -> FlowSessions).
    pub interact_sessions: Arc<RwLock<HashMap<String, FlowSessions>>>,
    /// Path to `sessions.yaml` for write-through persistence.
    pub sessions_path: PathBuf,
    /// Base data directory (~/.cthulu) for attachments etc.
    pub data_dir: PathBuf,
    /// Path to the `static/` directory (template YAML files live in `static/workflows/`).
    pub static_dir: PathBuf,
    /// Persistent Claude CLI processes keyed by session key (flow_id::node_id).
    pub live_processes: Arc<Mutex<HashMap<String, LiveClaudeProcess>>>,
    /// PTY-based Claude Code processes keyed by agent key (agent::{agent_id}).
    pub pty_processes: Arc<Mutex<HashMap<String, PtyProcess>>>,
    /// Sandbox provider for isolated executor runs.
    pub sandbox_provider: Arc<dyn SandboxProvider>,
    /// VM Manager provider (only set when using VmManager backend).
    /// Stored separately because the VM endpoints need VmManagerProvider-specific methods.
    pub vm_manager: Option<Arc<VmManagerProvider>>,
    /// VM-to-node mappings persisted in sessions.yaml. Key: "flow_id::node_id".
    pub vm_mappings: Arc<RwLock<HashMap<String, VmMapping>>>,
    /// Claude OAuth access token (read from macOS Keychain or CLAUDE_CODE_OAUTH_TOKEN env).
    /// Wrapped in Arc<RwLock> so it can be refreshed at runtime without a restart.
    pub oauth_token: Arc<RwLock<Option<String>>>,
    /// Live broadcast channels for flow-run session streaming.
    /// Key: session_id, Value: sender that broadcasts JSONL lines.
    pub session_streams: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
    /// In-memory event buffers for agent chat reconnection.
    /// Key: process_key (agent::{id}::session::{sid}), Value: buffered SSE events for current turn.
    pub chat_event_buffers: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl AppState {
    /// Save sessions + VM mappings to sessions.yaml.
    /// Reads vm_mappings synchronously (no await) via try_read to avoid async in some call sites.
    pub fn save_sessions_with_vms(&self, sessions: &HashMap<String, FlowSessions>) {
        let vms = self.vm_mappings.try_read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        save_sessions(&self.sessions_path, sessions, &vms);
    }
}

pub fn create_app(state: AppState) -> Router {
    routes::build_router(state)
}

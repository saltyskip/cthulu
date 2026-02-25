pub mod flow_routes;
pub mod middleware;
pub mod prompt_routes;
pub mod routes;

use axum::Router;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::flows::events::RunEvent;
use crate::flows::scheduler::FlowScheduler;
use crate::flows::store::Store;
use crate::github::client::GithubClient;
use crate::sandbox::backends::vm_manager::VmManagerProvider;
use crate::sandbox::provider::SandboxProvider;

/// Build a composite key for node-scoped sessions: `"flow_id::node_id"`.
pub fn node_sessions_key(flow_id: &str, node_id: &str) -> String {
    format!("{flow_id}::{node_id}")
}

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
    /// Number of messages exchanged in this session.
    pub message_count: u64,
    /// Cumulative cost (parsed from claude result events).
    pub total_cost: f64,
    /// When this session was created (ISO 8601).
    pub created_at: String,
    /// Path to the .skills/ directory for this session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills_dir: Option<String>,
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

/// Root structure for `sessions.yaml`.
#[derive(Debug, Serialize, Deserialize)]
struct SessionsFile {
    sessions: HashMap<String, FlowSessions>,
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

/// Load persisted sessions from a YAML file.
/// Supports auto-migration from the old single-session-per-flow format.
/// Returns an empty map if the file doesn't exist or can't be parsed.
pub fn load_sessions(path: &Path) -> HashMap<String, FlowSessions> {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %path.display(), error = %e, "failed to read sessions file");
            }
            return HashMap::new();
        }
    };

    // Try new format first
    if let Ok(file) = serde_yaml::from_str::<SessionsFile>(&contents) {
        tracing::info!(count = file.sessions.len(), "loaded persisted sessions");
        return file.sessions;
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
                    message_count: old.message_count,
                    total_cost: old.total_cost,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    skills_dir: None,
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
        return migrated;
    }

    tracing::warn!(path = %path.display(), "failed to parse sessions file in any known format");
    HashMap::new()
}

/// Persist sessions to a YAML file (atomic write via temp + rename).
pub fn save_sessions(path: &Path, sessions: &HashMap<String, FlowSessions>) {
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
                        message_count: s.message_count,
                        total_cost: s.total_cost,
                        created_at: s.created_at.clone(),
                        skills_dir: s.skills_dir.clone(),
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

#[derive(Clone)]
pub struct AppState {
    pub github_client: Option<Arc<dyn GithubClient>>,
    pub http_client: Arc<reqwest::Client>,
    pub store: Arc<dyn Store>,
    pub scheduler: Arc<FlowScheduler>,
    pub events_tx: broadcast::Sender<RunEvent>,
    /// Per-workflow session lists (flow_id -> FlowSessions).
    pub interact_sessions: Arc<RwLock<HashMap<String, FlowSessions>>>,
    /// Path to `sessions.yaml` for write-through persistence.
    pub sessions_path: PathBuf,
    /// Base data directory (~/.cthulu) for attachments etc.
    pub data_dir: PathBuf,
    /// Persistent Claude CLI processes keyed by session key (flow_id::node_id).
    pub live_processes: Arc<Mutex<HashMap<String, LiveClaudeProcess>>>,
    /// Sandbox provider for isolated executor runs.
    pub sandbox_provider: Arc<dyn SandboxProvider>,
    /// VM Manager provider (only set when using VmManager backend).
    /// Stored separately because the VM endpoints need VmManagerProvider-specific methods.
    pub vm_manager: Option<Arc<VmManagerProvider>>,
}

pub fn create_app(state: AppState) -> Router {
    routes::build_router(state)
}

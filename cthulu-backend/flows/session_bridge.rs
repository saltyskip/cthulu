use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock, Mutex};

use crate::api::{FlowSessions, VmMapping};

/// Metadata linking a session to a flow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowRunMeta {
    pub flow_id: String,
    pub flow_name: String,
    pub run_id: String,
    pub node_id: String,
    pub node_label: String,
}

/// Lightweight bridge carrying only the Arc-wrapped fields that the executor
/// needs to create and update sessions. This avoids requiring the full `AppState`
/// inside `tokio::spawn` / `NodeDeps`.
#[derive(Clone)]
pub struct SessionBridge {
    /// Per-agent session pools (keyed by `agent::{agent_id}`).
    pub sessions: Arc<RwLock<HashMap<String, FlowSessions>>>,
    /// Path to `sessions.yaml` for atomic persistence.
    pub sessions_path: PathBuf,
    /// VM mappings for save_sessions helper.
    pub vm_mappings: Arc<RwLock<HashMap<String, VmMapping>>>,
    /// Base data directory (~/.cthulu).
    pub data_dir: PathBuf,
    /// Live broadcast channels for flow-run session streaming.
    /// Key: session_id, Value: sender that broadcasts JSONL lines.
    pub session_streams: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
}

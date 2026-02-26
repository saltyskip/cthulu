pub mod file_repository;
pub mod repository;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A reusable agent definition — owns the "what" (prompt, permissions, personality).
/// The execution environment ("where") stays on the executor node config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Inline prompt text or path to a .md file.
    pub prompt: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append_system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

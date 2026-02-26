pub mod file_repository;
pub mod repository;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPrompt {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub source_flow_name: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

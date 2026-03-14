use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub assignee_agent_id: String,
    /// Who created this task. "user" for UI-created tasks.
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// File-based task store
// ---------------------------------------------------------------------------

pub struct TaskFileStore {
    dir: PathBuf,
    cache: Arc<RwLock<HashMap<String, Task>>>,
}

impl TaskFileStore {
    /// Create a new store rooted at `data_dir/tasks/`.
    /// Loads all existing tasks from disk into the in-memory cache.
    pub async fn new(data_dir: &Path) -> Self {
        let dir = data_dir.join("tasks");
        let _ = tokio::fs::create_dir_all(&dir).await;

        let store = Self {
            dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
        };
        store.load_all().await;
        store
    }

    /// Scan the tasks directory and load all JSON files into cache.
    async fn load_all(&self) {
        let mut tasks = HashMap::new();
        if let Ok(mut entries) = tokio::fs::read_dir(&self.dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = tokio::fs::read_to_string(&path).await {
                        if let Ok(task) = serde_json::from_str::<Task>(&content) {
                            tasks.insert(task.id.clone(), task);
                        }
                    }
                }
            }
        }
        let count = tasks.len();
        *self.cache.write().await = tasks;
        if count > 0 {
            tracing::info!(count, "loaded tasks from disk");
        }
    }

    /// List all tasks, newest first.
    pub async fn list(&self) -> Vec<Task> {
        let cache = self.cache.read().await;
        let mut tasks: Vec<Task> = cache.values().cloned().collect();
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        tasks
    }

    /// List tasks assigned to a specific agent, newest first.
    pub async fn list_for_agent(&self, agent_id: &str) -> Vec<Task> {
        let cache = self.cache.read().await;
        let mut tasks: Vec<Task> = cache
            .values()
            .filter(|t| t.assignee_agent_id == agent_id)
            .cloned()
            .collect();
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        tasks
    }

    /// Get a single task by ID.
    pub async fn get(&self, id: &str) -> Option<Task> {
        self.cache.read().await.get(id).cloned()
    }

    /// Save (create or update) a task. Uses atomic tmp+rename.
    pub async fn save(&self, task: Task) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&task)
            .map_err(|e| format!("serialize task: {e}"))?;
        let path = self.dir.join(format!("{}.json", task.id));
        let tmp = path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &json)
            .await
            .map_err(|e| format!("write task: {e}"))?;
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(|e| format!("rename task: {e}"))?;
        self.cache.write().await.insert(task.id.clone(), task);
        Ok(())
    }

    /// Delete a task by ID. Returns true if it existed.
    pub async fn delete(&self, id: &str) -> Result<bool, String> {
        let path = self.dir.join(format!("{id}.json"));
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| format!("delete task: {e}"))?;
            self.cache.write().await.remove(id);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

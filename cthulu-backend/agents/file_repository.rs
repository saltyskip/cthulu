use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;

use super::Agent;
use super::repository::AgentRepository;

/// File-based CRUD store for agents. Mirrors `FileFlowRepository` patterns:
/// in-memory `RwLock<HashMap>` backed by JSON files at `~/.cthulu/agents/`.
pub struct FileAgentRepository {
    agents: RwLock<HashMap<String, Agent>>,
    dir: PathBuf,
    /// Filenames written by this process — used to skip fs-watcher events for our own writes.
    self_writes: std::sync::Mutex<HashSet<String>>,
}

impl FileAgentRepository {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            dir: base_dir.as_ref().join("agents"),
            self_writes: std::sync::Mutex::new(HashSet::new()),
        }
    }

    /// Mark a filename as written by this process (for fs-watcher loop prevention).
    pub fn mark_self_write(&self, filename: &str) {
        self.self_writes.lock().unwrap().insert(filename.to_string());
    }

    /// Consume (remove) a self-write marker. Returns true if the filename was present.
    pub fn consume_self_write(&self, filename: &str) -> bool {
        self.self_writes.lock().unwrap().remove(filename)
    }

    /// Re-read a single agent JSON file from disk into the cache. Returns the resource ID if successful.
    pub async fn reload_file(&self, filename: &str) -> Option<String> {
        let path = self.dir.join(filename);
        let content = std::fs::read_to_string(&path).ok()?;
        let agent: Agent = serde_json::from_str(&content).ok()?;
        let id = agent.id.clone();
        self.agents.write().await.insert(id.clone(), agent);
        tracing::debug!(agent_id = %id, filename, "reloaded agent from disk");
        Some(id)
    }

    /// Remove an agent from the cache by filename. Returns the resource ID if it was present.
    pub async fn evict_file(&self, filename: &str) -> Option<String> {
        let id = filename.trim_end_matches(".json").to_string();
        let removed = self.agents.write().await.remove(&id);
        if removed.is_some() {
            tracing::debug!(agent_id = %id, filename, "evicted agent from cache");
        }
        removed.map(|a| a.id)
    }
}

#[async_trait]
impl AgentRepository for FileAgentRepository {
    async fn list(&self) -> Vec<Agent> {
        self.agents.read().await.values().cloned().collect()
    }

    async fn get(&self, id: &str) -> Option<Agent> {
        self.agents.read().await.get(id).cloned()
    }

    async fn save(&self, agent: Agent) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let filename = format!("{}.json", agent.id);
        self.mark_self_write(&filename);
        let path = self.dir.join(&filename);
        let content = serde_json::to_string_pretty(&agent)?;

        // Atomic write via temp file + rename
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, content)?;
        std::fs::rename(&tmp_path, &path)?;

        self.agents.write().await.insert(agent.id.clone(), agent);
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let filename = format!("{id}.json");
        self.mark_self_write(&filename);
        let existed = self.agents.write().await.remove(id).is_some();
        let path = self.dir.join(&filename);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(existed)
    }

    /// Load all agent JSON files from disk into the in-memory map.
    async fn load_all(&self) -> Result<()> {
        if !self.dir.exists() {
            std::fs::create_dir_all(&self.dir)?;
            return Ok(());
        }

        let mut map = HashMap::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Agent>(&content) {
                    Ok(agent) => {
                        map.insert(agent.id.clone(), agent);
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "failed to parse agent file");
                    }
                },
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "failed to read agent file");
                }
            }
        }

        tracing::info!(count = map.len(), "loaded agents");
        *self.agents.write().await = map;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_crud() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FileAgentRepository::new(tmp.path());
        store.load_all().await.unwrap();

        // Create
        let agent = Agent::builder("test-1")
            .name("Test Agent")
            .description("A test agent")
            .prompt("You are a helpful assistant.")
            .permissions(vec!["Read".to_string()])
            .build();
        store.save(agent.clone()).await.unwrap();

        // List
        let agents = store.list().await;
        assert_eq!(agents.len(), 1);

        // Get
        let fetched = store.get("test-1").await.unwrap();
        assert_eq!(fetched.name, "Test Agent");

        // Update
        let mut updated = fetched;
        updated.name = "Updated Agent".to_string();
        store.save(updated).await.unwrap();
        let fetched = store.get("test-1").await.unwrap();
        assert_eq!(fetched.name, "Updated Agent");

        // Delete
        let existed = store.delete("test-1").await.unwrap();
        assert!(existed);
        assert!(store.get("test-1").await.is_none());
        assert!(store.list().await.is_empty());

        // Persistence: reload from disk
        let store2 = FileAgentRepository::new(tmp.path());
        store2.load_all().await.unwrap();
        assert!(store2.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_agent_persistence() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FileAgentRepository::new(tmp.path());
        store.load_all().await.unwrap();

        let agent = Agent::builder("persist-1")
            .name("Persistent")
            .prompt("Do stuff")
            .append_system_prompt("Be brief.")
            .working_dir("/tmp")
            .build();
        store.save(agent).await.unwrap();

        // New store instance, load from disk
        let store2 = FileAgentRepository::new(tmp.path());
        store2.load_all().await.unwrap();
        let loaded = store2.get("persist-1").await.unwrap();
        assert_eq!(loaded.name, "Persistent");
        assert_eq!(loaded.append_system_prompt.as_deref(), Some("Be brief."));
        assert_eq!(loaded.working_dir.as_deref(), Some("/tmp"));
    }
}

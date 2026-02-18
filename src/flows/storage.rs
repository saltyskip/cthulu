use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::sync::RwLock;

use super::Flow;

pub struct FlowStore {
    flows_dir: PathBuf,
    flows: RwLock<HashMap<String, Flow>>,
}

impl FlowStore {
    pub fn new(flows_dir: PathBuf) -> Self {
        Self {
            flows_dir,
            flows: RwLock::new(HashMap::new()),
        }
    }

    pub fn flows_dir(&self) -> &PathBuf {
        &self.flows_dir
    }

    pub async fn load_all(&self) -> Result<()> {
        std::fs::create_dir_all(&self.flows_dir).with_context(|| {
            format!(
                "failed to create flows directory: {}",
                self.flows_dir.display()
            )
        })?;

        let mut loaded = HashMap::new();

        let entries = std::fs::read_dir(&self.flows_dir).with_context(|| {
            format!("failed to read flows directory: {}", self.flows_dir.display())
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read flow file: {}", path.display()))?;

            let flow: Flow = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse flow file: {}", path.display()))?;

            tracing::info!(flow_id = %flow.id, name = %flow.name, "Loaded flow");
            loaded.insert(flow.id.clone(), flow);
        }

        let count = loaded.len();
        *self.flows.write().await = loaded;
        tracing::info!(count, "Loaded all flows");

        Ok(())
    }

    pub async fn list(&self) -> Vec<Flow> {
        self.flows.read().await.values().cloned().collect()
    }

    pub async fn get(&self, id: &str) -> Option<Flow> {
        self.flows.read().await.get(id).cloned()
    }

    pub async fn save(&self, flow: Flow) -> Result<()> {
        let path = self.flows_dir.join(format!("{}.json", flow.id));
        let content = serde_json::to_string_pretty(&flow)
            .context("failed to serialize flow")?;

        std::fs::write(&path, content)
            .with_context(|| format!("failed to write flow file: {}", path.display()))?;

        self.flows.write().await.insert(flow.id.clone(), flow);
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let path = self.flows_dir.join(format!("{id}.json"));

        let existed = self.flows.write().await.remove(id).is_some();

        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to delete flow file: {}", path.display()))?;
        }

        Ok(existed)
    }

    pub async fn is_empty(&self) -> bool {
        self.flows.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::{Node, NodeType, Position};
    use chrono::Utc;
    use tempfile::tempdir;

    fn test_flow(id: &str, name: &str) -> Flow {
        Flow {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            enabled: true,
            nodes: vec![Node {
                id: "n1".to_string(),
                node_type: NodeType::Trigger,
                kind: "cron".to_string(),
                config: serde_json::json!({"schedule": "0 * * * *"}),
                position: Position { x: 0.0, y: 0.0 },
                label: "Cron".to_string(),
            }],
            edges: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let dir = tempdir().unwrap();
        let store = FlowStore::new(dir.path().to_path_buf());
        store.load_all().await.unwrap();

        let flow = test_flow("f1", "Test Flow");
        store.save(flow).await.unwrap();

        // Create a new store and reload
        let store2 = FlowStore::new(dir.path().to_path_buf());
        store2.load_all().await.unwrap();

        let loaded = store2.get("f1").await.unwrap();
        assert_eq!(loaded.name, "Test Flow");
    }

    #[tokio::test]
    async fn test_delete() {
        let dir = tempdir().unwrap();
        let store = FlowStore::new(dir.path().to_path_buf());
        store.load_all().await.unwrap();

        store.save(test_flow("f1", "Flow 1")).await.unwrap();
        assert!(!store.is_empty().await);

        let deleted = store.delete("f1").await.unwrap();
        assert!(deleted);
        assert!(store.is_empty().await);
        assert!(store.get("f1").await.is_none());

        // File should be gone
        assert!(!dir.path().join("f1.json").exists());
    }

    #[tokio::test]
    async fn test_list() {
        let dir = tempdir().unwrap();
        let store = FlowStore::new(dir.path().to_path_buf());
        store.load_all().await.unwrap();

        store.save(test_flow("f1", "Flow 1")).await.unwrap();
        store.save(test_flow("f2", "Flow 2")).await.unwrap();

        let flows = store.list().await;
        assert_eq!(flows.len(), 2);
    }
}

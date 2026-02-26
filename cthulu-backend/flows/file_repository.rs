use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use super::Flow;
use super::history::{FlowRun, NodeRun, RunStatus, MAX_RUNS_PER_FLOW};
use super::repository::FlowRepository;

pub struct FileFlowRepository {
    base_dir: PathBuf,
    flows: RwLock<HashMap<String, Flow>>,
    runs: RwLock<HashMap<String, VecDeque<FlowRun>>>,
}

impl FileFlowRepository {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            flows: RwLock::new(HashMap::new()),
            runs: RwLock::new(HashMap::new()),
        }
    }

    fn flows_dir(&self) -> PathBuf {
        self.base_dir.join("flows")
    }

    fn runs_dir(&self) -> PathBuf {
        self.base_dir.join("runs")
    }

    fn run_file(&self, flow_id: &str, run_id: &str) -> PathBuf {
        self.runs_dir().join(flow_id).join(format!("{run_id}.json"))
    }

    pub fn attachments_dir(&self, flow_id: &str, node_id: &str) -> PathBuf {
        self.base_dir.join("attachments").join(flow_id).join(node_id)
    }

    fn flush_run(&self, flow_id: &str, run: &FlowRun) -> Result<()> {
        let dir = self.runs_dir().join(flow_id);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create runs dir: {}", dir.display()))?;
        let path = dir.join(format!("{}.json", run.id));
        let content = serde_json::to_string_pretty(run)
            .context("failed to serialize run")?;
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write run file: {}", path.display()))?;
        Ok(())
    }

    /// Find a mutable reference to a run and flush it after mutation.
    async fn mutate_run<F>(&self, flow_id: &str, run_id: &str, mutate: F) -> Result<()>
    where
        F: FnOnce(&mut FlowRun),
    {
        let mut runs = self.runs.write().await;
        if let Some(queue) = runs.get_mut(flow_id) {
            if let Some(run) = queue.iter_mut().find(|r| r.id == run_id) {
                mutate(run);
                self.flush_run(flow_id, run)?;
                return Ok(());
            }
        }
        bail!("run {run_id} not found for flow {flow_id}")
    }
}

#[async_trait]
impl FlowRepository for FileFlowRepository {
    async fn list_flows(&self) -> Vec<Flow> {
        self.flows.read().await.values().cloned().collect()
    }

    async fn get_flow(&self, id: &str) -> Option<Flow> {
        self.flows.read().await.get(id).cloned()
    }

    async fn save_flow(&self, flow: Flow) -> Result<()> {
        let dir = self.flows_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create flows dir: {}", dir.display()))?;

        let path = dir.join(format!("{}.json", flow.id));
        let content = serde_json::to_string_pretty(&flow)
            .context("failed to serialize flow")?;
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write flow file: {}", path.display()))?;

        self.flows.write().await.insert(flow.id.clone(), flow);
        Ok(())
    }

    async fn delete_flow(&self, id: &str) -> Result<bool> {
        let flow_path = self.flows_dir().join(format!("{id}.json"));
        let existed = self.flows.write().await.remove(id).is_some();

        if flow_path.exists() {
            std::fs::remove_file(&flow_path)
                .with_context(|| format!("failed to delete flow file: {}", flow_path.display()))?;
        }

        // Clean up runs for this flow
        self.runs.write().await.remove(id);
        let runs_path = self.runs_dir().join(id);
        if runs_path.exists() {
            std::fs::remove_dir_all(&runs_path)
                .with_context(|| format!("failed to delete runs dir: {}", runs_path.display()))?;
        }

        Ok(existed)
    }

    async fn add_run(&self, run: FlowRun) -> Result<()> {
        self.flush_run(&run.flow_id, &run)?;

        let mut runs = self.runs.write().await;
        let queue = runs.entry(run.flow_id.clone()).or_default();
        queue.push_back(run);

        // Enforce cap
        while queue.len() > MAX_RUNS_PER_FLOW {
            if let Some(old) = queue.pop_front() {
                let path = self.run_file(&old.flow_id, &old.id);
                let _ = std::fs::remove_file(path);
            }
        }

        Ok(())
    }

    async fn get_runs(&self, flow_id: &str, limit: usize) -> Vec<FlowRun> {
        let runs = self.runs.read().await;
        runs.get(flow_id)
            .map(|q| q.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    async fn complete_run(
        &self,
        flow_id: &str,
        run_id: &str,
        status: RunStatus,
        error: Option<String>,
    ) -> Result<()> {
        self.mutate_run(flow_id, run_id, |r| {
            r.status = status;
            r.finished_at = Some(Utc::now());
            r.error = error;
        })
        .await
    }

    async fn push_node_run(
        &self,
        flow_id: &str,
        run_id: &str,
        node_run: NodeRun,
    ) -> Result<()> {
        self.mutate_run(flow_id, run_id, |r| {
            r.node_runs.push(node_run);
        })
        .await
    }

    async fn complete_node_run(
        &self,
        flow_id: &str,
        run_id: &str,
        node_id: &str,
        status: RunStatus,
        output_preview: Option<String>,
    ) -> Result<()> {
        let node_id = node_id.to_string();
        self.mutate_run(flow_id, run_id, |r| {
            if let Some(nr) = r.node_runs.iter_mut().find(|nr| nr.node_id == node_id) {
                nr.status = status;
                nr.finished_at = Some(Utc::now());
                nr.output_preview = output_preview;
            }
        })
        .await
    }

    async fn load_all(&self) -> Result<()> {
        // Load flows
        let flows_dir = self.flows_dir();
        std::fs::create_dir_all(&flows_dir)
            .with_context(|| format!("failed to create flows dir: {}", flows_dir.display()))?;

        let mut loaded_flows = HashMap::new();
        let entries = std::fs::read_dir(&flows_dir)
            .with_context(|| format!("failed to read flows dir: {}", flows_dir.display()))?;

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
            loaded_flows.insert(flow.id.clone(), flow);
        }

        let flow_count = loaded_flows.len();
        *self.flows.write().await = loaded_flows;
        tracing::info!(count = flow_count, "Loaded all flows");

        // Load runs
        let runs_dir = self.runs_dir();
        std::fs::create_dir_all(&runs_dir)
            .with_context(|| format!("failed to create runs dir: {}", runs_dir.display()))?;

        let mut loaded_runs: HashMap<String, VecDeque<FlowRun>> = HashMap::new();

        let flow_dirs = std::fs::read_dir(&runs_dir)
            .with_context(|| format!("failed to read runs dir: {}", runs_dir.display()))?;

        for flow_dir_entry in flow_dirs {
            let flow_dir_entry = flow_dir_entry?;
            if !flow_dir_entry.file_type()?.is_dir() {
                continue;
            }
            let flow_id = flow_dir_entry
                .file_name()
                .to_string_lossy()
                .to_string();

            let mut flow_runs = Vec::new();
            let run_entries = std::fs::read_dir(flow_dir_entry.path())?;

            for run_entry in run_entries {
                let run_entry = run_entry?;
                let path = run_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read run file: {}", path.display()))?;
                match serde_json::from_str::<FlowRun>(&content) {
                    Ok(run) => flow_runs.push(run),
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "Skipping invalid run file");
                        continue;
                    }
                }
            }

            // Sort by started_at ascending
            flow_runs.sort_by(|a, b| a.started_at.cmp(&b.started_at));

            // Enforce cap: delete overflow files
            while flow_runs.len() > MAX_RUNS_PER_FLOW {
                let old = flow_runs.remove(0);
                let path = self.run_file(&flow_id, &old.id);
                let _ = std::fs::remove_file(path);
            }

            let run_count = flow_runs.len();
            if run_count > 0 {
                tracing::info!(flow_id = %flow_id, count = run_count, "Loaded runs");
            }
            loaded_runs.insert(flow_id, VecDeque::from(flow_runs));
        }

        *self.runs.write().await = loaded_runs;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::{Node, NodeType, Position};
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

    fn test_run(flow_id: &str, run_id: &str) -> FlowRun {
        FlowRun {
            id: run_id.to_string(),
            flow_id: flow_id.to_string(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            node_runs: vec![],
            error: None,
        }
    }

    // ── Flow CRUD ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_flow_save_and_load() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        let flow = test_flow("f1", "Test Flow");
        repo.save_flow(flow).await.unwrap();

        // New repo on same dir
        let repo2 = FileFlowRepository::new(dir.path().to_path_buf());
        repo2.load_all().await.unwrap();

        let loaded = repo2.get_flow("f1").await.unwrap();
        assert_eq!(loaded.name, "Test Flow");
    }

    #[tokio::test]
    async fn test_flow_list() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        repo.save_flow(test_flow("f1", "Flow 1")).await.unwrap();
        repo.save_flow(test_flow("f2", "Flow 2")).await.unwrap();

        let flows = repo.list_flows().await;
        assert_eq!(flows.len(), 2);
    }

    #[tokio::test]
    async fn test_flow_delete() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        repo.save_flow(test_flow("f1", "Flow 1")).await.unwrap();

        // Add a run for this flow
        repo.add_run(test_run("f1", "r1")).await.unwrap();

        let deleted = repo.delete_flow("f1").await.unwrap();
        assert!(deleted);
        assert!(repo.get_flow("f1").await.is_none());

        // Runs dir should be cleaned up
        assert!(!dir.path().join("runs").join("f1").exists());
    }

    // ── Run persistence ──────────────────────────────────────────

    #[tokio::test]
    async fn test_run_persistence_across_restart() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        repo.add_run(test_run("f1", "r1")).await.unwrap();
        repo.add_run(test_run("f1", "r2")).await.unwrap();
        drop(repo);

        // New repo on same dir
        let repo2 = FileFlowRepository::new(dir.path().to_path_buf());
        repo2.load_all().await.unwrap();

        let runs = repo2.get_runs("f1", 100).await;
        assert_eq!(runs.len(), 2);
        // Newest first
        assert_eq!(runs[0].id, "r2");
        assert_eq!(runs[1].id, "r1");
    }

    #[tokio::test]
    async fn test_run_cap_enforced() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        for i in 0..105 {
            let mut run = test_run("f1", &format!("r{i}"));
            // Ensure distinct started_at for ordering
            run.started_at = Utc::now() + chrono::Duration::milliseconds(i as i64);
            repo.add_run(run).await.unwrap();
        }

        let runs = repo.get_runs("f1", 200).await;
        assert_eq!(runs.len(), MAX_RUNS_PER_FLOW);

        // Verify on disk too
        let run_dir = dir.path().join("runs").join("f1");
        let count = std::fs::read_dir(&run_dir).unwrap().count();
        assert_eq!(count, MAX_RUNS_PER_FLOW);
    }

    #[tokio::test]
    async fn test_run_cap_enforced_on_load() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        // Write 105 run files directly (bypassing cap)
        let runs_dir = dir.path().join("runs").join("f1");
        std::fs::create_dir_all(&runs_dir).unwrap();
        for i in 0..105 {
            let mut run = test_run("f1", &format!("r{i:03}"));
            run.started_at = Utc::now() + chrono::Duration::milliseconds(i as i64);
            let content = serde_json::to_string_pretty(&run).unwrap();
            std::fs::write(runs_dir.join(format!("r{i:03}.json")), content).unwrap();
        }

        // Load should trim to 100
        let repo2 = FileFlowRepository::new(dir.path().to_path_buf());
        repo2.load_all().await.unwrap();

        let runs = repo2.get_runs("f1", 200).await;
        assert_eq!(runs.len(), MAX_RUNS_PER_FLOW);

        let count = std::fs::read_dir(&runs_dir).unwrap().count();
        assert_eq!(count, MAX_RUNS_PER_FLOW);
    }

    // ── Run mutations ────────────────────────────────────────────

    #[tokio::test]
    async fn test_complete_run() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        repo.add_run(test_run("f1", "r1")).await.unwrap();
        repo
            .complete_run("f1", "r1", RunStatus::Success, None)
            .await
            .unwrap();

        let runs = repo.get_runs("f1", 10).await;
        assert_eq!(runs[0].status, RunStatus::Success);
        assert!(runs[0].finished_at.is_some());

        // Verify flushed to disk
        drop(repo);
        let repo2 = FileFlowRepository::new(dir.path().to_path_buf());
        repo2.load_all().await.unwrap();
        let runs = repo2.get_runs("f1", 10).await;
        assert_eq!(runs[0].status, RunStatus::Success);
    }

    #[tokio::test]
    async fn test_push_node_run() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        repo.add_run(test_run("f1", "r1")).await.unwrap();

        let nr = NodeRun {
            node_id: "n1".to_string(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            output_preview: None,
        };
        repo.push_node_run("f1", "r1", nr).await.unwrap();

        let runs = repo.get_runs("f1", 10).await;
        assert_eq!(runs[0].node_runs.len(), 1);
        assert_eq!(runs[0].node_runs[0].node_id, "n1");
    }

    #[tokio::test]
    async fn test_complete_node_run() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        repo.add_run(test_run("f1", "r1")).await.unwrap();

        let nr = NodeRun {
            node_id: "n1".to_string(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            output_preview: None,
        };
        repo.push_node_run("f1", "r1", nr).await.unwrap();
        repo
            .complete_node_run("f1", "r1", "n1", RunStatus::Success, Some("done".to_string()))
            .await
            .unwrap();

        let runs = repo.get_runs("f1", 10).await;
        assert_eq!(runs[0].node_runs[0].status, RunStatus::Success);
        assert_eq!(
            runs[0].node_runs[0].output_preview.as_deref(),
            Some("done")
        );
        assert!(runs[0].node_runs[0].finished_at.is_some());

        // Verify persistence
        drop(repo);
        let repo2 = FileFlowRepository::new(dir.path().to_path_buf());
        repo2.load_all().await.unwrap();
        let runs = repo2.get_runs("f1", 10).await;
        assert_eq!(runs[0].node_runs[0].status, RunStatus::Success);
    }

    #[tokio::test]
    async fn test_complete_run_with_error() {
        let dir = tempdir().unwrap();
        let repo = FileFlowRepository::new(dir.path().to_path_buf());
        repo.load_all().await.unwrap();

        repo.add_run(test_run("f1", "r1")).await.unwrap();
        repo
            .complete_run("f1", "r1", RunStatus::Failed, Some("boom".to_string()))
            .await
            .unwrap();

        let runs = repo.get_runs("f1", 10).await;
        assert_eq!(runs[0].status, RunStatus::Failed);
        assert_eq!(runs[0].error.as_deref(), Some("boom"));
    }
}

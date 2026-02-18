use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::RwLock;

const MAX_RUNS_PER_FLOW: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowRun {
    pub id: String,
    pub flow_id: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub node_runs: Vec<NodeRun>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeRun {
    pub node_id: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub output_preview: Option<String>,
}

pub struct RunHistory {
    runs: RwLock<HashMap<String, VecDeque<FlowRun>>>,
}

impl RunHistory {
    pub fn new() -> Self {
        Self {
            runs: RwLock::new(HashMap::new()),
        }
    }

    pub async fn add_run(&self, run: FlowRun) {
        let mut runs = self.runs.write().await;
        let queue = runs.entry(run.flow_id.clone()).or_default();
        queue.push_back(run);
        while queue.len() > MAX_RUNS_PER_FLOW {
            queue.pop_front();
        }
    }

    pub async fn update_run<F>(&self, flow_id: &str, run_id: &str, update: F)
    where
        F: FnOnce(&mut FlowRun),
    {
        let mut runs = self.runs.write().await;
        if let Some(queue) = runs.get_mut(flow_id) {
            if let Some(run) = queue.iter_mut().find(|r| r.id == run_id) {
                update(run);
            }
        }
    }

    pub async fn get_runs(&self, flow_id: &str) -> Vec<FlowRun> {
        let runs = self.runs.read().await;
        runs.get(flow_id)
            .map(|q| q.iter().rev().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn test_add_and_get_runs() {
        let history = RunHistory::new();
        history.add_run(test_run("f1", "r1")).await;
        history.add_run(test_run("f1", "r2")).await;

        let runs = history.get_runs("f1").await;
        assert_eq!(runs.len(), 2);
        // Most recent first
        assert_eq!(runs[0].id, "r2");
        assert_eq!(runs[1].id, "r1");
    }

    #[tokio::test]
    async fn test_update_run() {
        let history = RunHistory::new();
        history.add_run(test_run("f1", "r1")).await;

        history
            .update_run("f1", "r1", |run| {
                run.status = RunStatus::Success;
                run.finished_at = Some(Utc::now());
            })
            .await;

        let runs = history.get_runs("f1").await;
        assert_eq!(runs[0].status, RunStatus::Success);
        assert!(runs[0].finished_at.is_some());
    }

    #[tokio::test]
    async fn test_cap_at_max() {
        let history = RunHistory::new();
        for i in 0..150 {
            history.add_run(test_run("f1", &format!("r{i}"))).await;
        }
        let runs = history.get_runs("f1").await;
        assert_eq!(runs.len(), MAX_RUNS_PER_FLOW);
    }
}

//! SQLite-backed repositories for agents, flows, and prompts.
//!
//! Each entity is stored as a JSON blob in a `data` column alongside indexed
//! `id` and `name` columns. This gives ACID transactions and concurrent reads
//! while preserving full fidelity for complex nested types (hooks, subagents).

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

use crate::agents::{Agent, repository::AgentRepository};
use crate::flows::{Flow, repository::FlowRepository};
use crate::flows::history::{FlowRun, NodeRun, RunStatus};
use crate::prompts::{SavedPrompt, repository::PromptRepository};

/// Shared SQLite connection wrapped in a Mutex for Send + Sync.
/// rusqlite::Connection is !Send, so we use a blocking Mutex and
/// run queries on `spawn_blocking` when called from async contexts.
pub struct SqliteDb {
    conn: Mutex<Connection>,
}

impl SqliteDb {
    /// Open (or create) a SQLite database at the given path and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open SQLite database at {}", path.display()))?;

        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agents (
                id   TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS flows (
                id   TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS flow_runs (
                id      TEXT NOT NULL,
                flow_id TEXT NOT NULL,
                data    TEXT NOT NULL,
                PRIMARY KEY (flow_id, id)
            );
            CREATE TABLE IF NOT EXISTS prompts (
                id    TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                data  TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
                key  TEXT PRIMARY KEY,
                data TEXT NOT NULL
            );",
        )?;

        Ok(Self { conn: Mutex::new(conn) })
    }
}

// ---------------------------------------------------------------------------
// AgentRepository
// ---------------------------------------------------------------------------

pub struct SqliteAgentRepository {
    db: std::sync::Arc<SqliteDb>,
}

impl SqliteAgentRepository {
    pub fn new(db: std::sync::Arc<SqliteDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AgentRepository for SqliteAgentRepository {
    async fn list(&self) -> Vec<Agent> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT data FROM agents ORDER BY name").unwrap();
            stmt.query_map([], |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str::<Agent>(&json).ok())
            })
            .unwrap()
            .flatten()
            .flatten()
            .collect()
        })
        .await
        .unwrap_or_default()
    }

    async fn get(&self, id: &str) -> Option<Agent> {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT data FROM agents WHERE id = ?1", [&id], |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str::<Agent>(&json).ok())
            })
            .ok()
            .flatten()
        })
        .await
        .unwrap_or(None)
    }

    async fn save(&self, agent: Agent) -> Result<()> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let json = serde_json::to_string(&agent)?;
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO agents (id, name, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![agent.id, agent.name, json],
            )?;
            Ok(())
        })
        .await?
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let rows = conn.execute("DELETE FROM agents WHERE id = ?1", [&id])?;
            Ok(rows > 0)
        })
        .await?
    }

    async fn load_all(&self) -> Result<()> {
        // No-op: SQLite data is always persisted and loaded on demand.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FlowRepository
// ---------------------------------------------------------------------------

pub struct SqliteFlowRepository {
    db: std::sync::Arc<SqliteDb>,
}

impl SqliteFlowRepository {
    pub fn new(db: std::sync::Arc<SqliteDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl FlowRepository for SqliteFlowRepository {
    async fn list_flows(&self) -> Vec<Flow> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT data FROM flows ORDER BY name").unwrap();
            stmt.query_map([], |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str::<Flow>(&json).ok())
            })
            .unwrap()
            .flatten()
            .flatten()
            .collect()
        })
        .await
        .unwrap_or_default()
    }

    async fn get_flow(&self, id: &str) -> Option<Flow> {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT data FROM flows WHERE id = ?1", [&id], |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str::<Flow>(&json).ok())
            })
            .ok()
            .flatten()
        })
        .await
        .unwrap_or(None)
    }

    async fn save_flow(&self, flow: Flow) -> Result<()> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let json = serde_json::to_string(&flow)?;
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO flows (id, name, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![flow.id, flow.name, json],
            )?;
            Ok(())
        })
        .await?
    }

    async fn delete_flow(&self, id: &str) -> Result<bool> {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let rows = conn.execute("DELETE FROM flows WHERE id = ?1", [&id])?;
            // Also clean up runs
            conn.execute("DELETE FROM flow_runs WHERE flow_id = ?1", [&id])?;
            Ok(rows > 0)
        })
        .await?
    }

    async fn add_run(&self, run: FlowRun) -> Result<()> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let json = serde_json::to_string(&run)?;
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO flow_runs (id, flow_id, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![run.id, run.flow_id, json],
            )?;
            Ok(())
        })
        .await?
    }

    async fn get_runs(&self, flow_id: &str, limit: usize) -> Vec<FlowRun> {
        let db = self.db.clone();
        let flow_id = flow_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT data FROM flow_runs WHERE flow_id = ?1 ORDER BY rowid DESC LIMIT ?2")
                .unwrap();
            stmt.query_map(rusqlite::params![flow_id, limit], |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str::<FlowRun>(&json).ok())
            })
            .unwrap()
            .flatten()
            .flatten()
            .collect()
        })
        .await
        .unwrap_or_default()
    }

    async fn complete_run(
        &self,
        flow_id: &str,
        run_id: &str,
        status: RunStatus,
        error: Option<String>,
    ) -> Result<()> {
        let db = self.db.clone();
        let flow_id = flow_id.to_string();
        let run_id = run_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let json: Option<String> = conn
                .query_row(
                    "SELECT data FROM flow_runs WHERE flow_id = ?1 AND id = ?2",
                    rusqlite::params![flow_id, run_id],
                    |row| row.get(0),
                )
                .ok();
            if let Some(json) = json {
                if let Ok(mut run) = serde_json::from_str::<FlowRun>(&json) {
                    run.status = status;
                    run.error = error;
                    run.finished_at = Some(chrono::Utc::now());
                    let updated = serde_json::to_string(&run)?;
                    conn.execute(
                        "UPDATE flow_runs SET data = ?1 WHERE flow_id = ?2 AND id = ?3",
                        rusqlite::params![updated, flow_id, run_id],
                    )?;
                }
            }
            Ok(())
        })
        .await?
    }

    async fn push_node_run(
        &self,
        flow_id: &str,
        run_id: &str,
        node_run: NodeRun,
    ) -> Result<()> {
        let db = self.db.clone();
        let flow_id = flow_id.to_string();
        let run_id = run_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let json: Option<String> = conn
                .query_row(
                    "SELECT data FROM flow_runs WHERE flow_id = ?1 AND id = ?2",
                    rusqlite::params![flow_id, run_id],
                    |row| row.get(0),
                )
                .ok();
            if let Some(json) = json {
                if let Ok(mut run) = serde_json::from_str::<FlowRun>(&json) {
                    run.node_runs.push(node_run);
                    let updated = serde_json::to_string(&run)?;
                    conn.execute(
                        "UPDATE flow_runs SET data = ?1 WHERE flow_id = ?2 AND id = ?3",
                        rusqlite::params![updated, flow_id, run_id],
                    )?;
                }
            }
            Ok(())
        })
        .await?
    }

    async fn complete_node_run(
        &self,
        flow_id: &str,
        run_id: &str,
        node_id: &str,
        status: RunStatus,
        output_preview: Option<String>,
    ) -> Result<()> {
        let db = self.db.clone();
        let flow_id = flow_id.to_string();
        let run_id = run_id.to_string();
        let node_id = node_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let json: Option<String> = conn
                .query_row(
                    "SELECT data FROM flow_runs WHERE flow_id = ?1 AND id = ?2",
                    rusqlite::params![flow_id, run_id],
                    |row| row.get(0),
                )
                .ok();
            if let Some(json) = json {
                if let Ok(mut run) = serde_json::from_str::<FlowRun>(&json) {
                    if let Some(nr) = run.node_runs.iter_mut().find(|n| n.node_id == node_id) {
                        nr.status = status;
                        nr.output_preview = output_preview;
                        nr.finished_at = Some(chrono::Utc::now());
                    }
                    let updated = serde_json::to_string(&run)?;
                    conn.execute(
                        "UPDATE flow_runs SET data = ?1 WHERE flow_id = ?2 AND id = ?3",
                        rusqlite::params![updated, flow_id, run_id],
                    )?;
                }
            }
            Ok(())
        })
        .await?
    }

    async fn load_all(&self) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PromptRepository
// ---------------------------------------------------------------------------

pub struct SqlitePromptRepository {
    db: std::sync::Arc<SqliteDb>,
}

impl SqlitePromptRepository {
    pub fn new(db: std::sync::Arc<SqliteDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl PromptRepository for SqlitePromptRepository {
    async fn list_prompts(&self) -> Vec<SavedPrompt> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT data FROM prompts ORDER BY title").unwrap();
            stmt.query_map([], |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str::<SavedPrompt>(&json).ok())
            })
            .unwrap()
            .flatten()
            .flatten()
            .collect()
        })
        .await
        .unwrap_or_default()
    }

    async fn get_prompt(&self, id: &str) -> Option<SavedPrompt> {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT data FROM prompts WHERE id = ?1", [&id], |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str::<SavedPrompt>(&json).ok())
            })
            .ok()
            .flatten()
        })
        .await
        .unwrap_or(None)
    }

    async fn save_prompt(&self, prompt: SavedPrompt) -> Result<()> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let json = serde_json::to_string(&prompt)?;
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO prompts (id, title, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![prompt.id, prompt.title, json],
            )?;
            Ok(())
        })
        .await?
    }

    async fn delete_prompt(&self, id: &str) -> Result<bool> {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn.lock().unwrap();
            let rows = conn.execute("DELETE FROM prompts WHERE id = ?1", [&id])?;
            Ok(rows > 0)
        })
        .await?
    }

    async fn load_all(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::Agent;
    use chrono::Utc;
    use std::collections::HashMap;

    fn test_db() -> std::sync::Arc<SqliteDb> {
        std::sync::Arc::new(SqliteDb::open(Path::new(":memory:")).unwrap())
    }

    fn test_agent(id: &str, name: &str) -> Agent {
        Agent::builder(id).name(name).prompt("test prompt").build()
    }

    #[tokio::test]
    async fn agent_crud() {
        let db = test_db();
        let repo = SqliteAgentRepository::new(db);

        // Empty initially
        assert!(repo.list().await.is_empty());
        assert!(repo.get("a1").await.is_none());

        // Save and retrieve
        repo.save(test_agent("a1", "Agent One")).await.unwrap();
        let got = repo.get("a1").await.unwrap();
        assert_eq!(got.name, "Agent One");

        // List
        repo.save(test_agent("a2", "Agent Two")).await.unwrap();
        assert_eq!(repo.list().await.len(), 2);

        // Update
        let mut updated = got.clone();
        updated.name = "Updated Agent".to_string();
        repo.save(updated).await.unwrap();
        assert_eq!(repo.get("a1").await.unwrap().name, "Updated Agent");

        // Delete
        assert!(repo.delete("a1").await.unwrap());
        assert!(repo.get("a1").await.is_none());
        assert!(!repo.delete("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn prompt_crud() {
        let db = test_db();
        let repo = SqlitePromptRepository::new(db);

        assert!(repo.list_prompts().await.is_empty());

        let prompt = SavedPrompt {
            id: "p1".to_string(),
            title: "Test Prompt".to_string(),
            summary: "A test".to_string(),
            source_flow_name: String::new(),
            tags: vec!["test".to_string()],
            created_at: Utc::now(),
        };

        repo.save_prompt(prompt).await.unwrap();
        assert_eq!(repo.list_prompts().await.len(), 1);
        assert_eq!(repo.get_prompt("p1").await.unwrap().title, "Test Prompt");
        assert!(repo.delete_prompt("p1").await.unwrap());
        assert!(repo.list_prompts().await.is_empty());
    }
}

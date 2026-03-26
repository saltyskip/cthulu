//! MongoDB-backed repositories for agents, flows, and prompts.
//!
//! Env: MONGODB_URI=mongodb://root:checkOne@localhost:27017
//!      MONGODB_DB=cthulu (default)

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::{FindOptions, IndexOptions, ReplaceOptions};
use mongodb::{Client, Collection, Database, IndexModel};
use serde_json::Value;

use crate::agents::{Agent, repository::AgentRepository};
use crate::flows::history::{FlowRun, NodeRun, RunStatus};
use crate::flows::{repository::FlowRepository, Flow};
use crate::prompts::{repository::PromptRepository, SavedPrompt};

pub struct MongoDb {
    db: Database,
}

impl MongoDb {
    pub async fn connect(uri: &str, db_name: &str) -> Result<Self> {
        let client = Client::with_uri_str(uri)
            .await
            .context("failed to connect to MongoDB")?;

        client
            .database("admin")
            .run_command(doc! { "ping": 1 }, None)
            .await
            .context("MongoDB ping failed")?;

        let db = client.database(db_name);

        // Create unique indexes
        let unique = IndexOptions::builder().unique(true).build();
        db.collection::<Document>("agents")
            .create_index(IndexModel::builder().keys(doc! { "id": 1 }).options(unique.clone()).build(), None)
            .await.ok();
        db.collection::<Document>("flows")
            .create_index(IndexModel::builder().keys(doc! { "id": 1 }).options(unique.clone()).build(), None)
            .await.ok();
        db.collection::<Document>("prompts")
            .create_index(IndexModel::builder().keys(doc! { "id": 1 }).options(unique).build(), None)
            .await.ok();
        db.collection::<Document>("sessions")
            .create_index(IndexModel::builder().keys(doc! { "key": 1 }).options(IndexOptions::builder().unique(true).build()).build(), None)
            .await.ok();
        db.collection::<Document>("flow_runs")
            .create_index(IndexModel::builder().keys(doc! { "flow_id": 1, "id": 1 }).build(), None)
            .await.ok();

        tracing::info!(db = db_name, "MongoDB connected");
        Ok(Self { db })
    }
}

impl MongoDb {
    /// Load all users from MongoDB.
    pub async fn load_users(&self) -> std::collections::HashMap<String, crate::api::local_auth::StoredUser> {
        let cursor = match coll(&self.db, "users").find(doc! {}, None).await {
            Ok(c) => c,
            Err(e) => { tracing::error!(error = %e, "failed to load users from mongo"); return Default::default(); }
        };
        let docs: Vec<Document> = cursor.try_collect().await.unwrap_or_default();
        let mut map = std::collections::HashMap::new();
        for doc in docs {
            if let Some(user) = from_doc::<crate::api::local_auth::StoredUser>(doc) {
                map.insert(user.email.clone(), user);
            }
        }
        tracing::info!(count = map.len(), "loaded users from MongoDB");
        map
    }

    /// Save all users to MongoDB.
    pub async fn save_users(&self, users: &std::collections::HashMap<String, crate::api::local_auth::StoredUser>) {
        let opts = ReplaceOptions::builder().upsert(true).build();
        for (email, user) in users {
            if let Ok(doc) = to_doc(user) {
                if let Err(e) = coll(&self.db, "users").replace_one(doc! { "email": email }, doc, opts.clone()).await {
                    tracing::error!(email = %email, error = %e, "failed to save user to mongo");
                }
            }
        }
    }

    /// Load all sessions from MongoDB.
    pub async fn load_sessions(&self) -> std::collections::HashMap<String, crate::api::FlowSessions> {
        let cursor = match coll(&self.db, "sessions").find(doc! {}, None).await {
            Ok(c) => c,
            Err(e) => { tracing::error!(error = %e, "failed to load sessions from mongo"); return Default::default(); }
        };
        let docs: Vec<Document> = cursor.try_collect().await.unwrap_or_default();
        let mut map = std::collections::HashMap::new();
        for doc in docs {
            if let (Some(key), Some(data)) = (
                doc.get_str("key").ok().map(String::from),
                doc.get_document("data").ok(),
            ) {
                if let Some(fs) = from_doc::<crate::api::FlowSessions>(data.clone()) {
                    map.insert(key, fs);
                }
            }
        }
        tracing::info!(count = map.len(), "loaded sessions from MongoDB");
        map
    }

    /// Save all sessions to MongoDB (full replace).
    pub async fn save_sessions(&self, sessions: &std::collections::HashMap<String, crate::api::FlowSessions>) {
        for (key, flow_sessions) in sessions {
            if let Ok(data_doc) = to_doc(flow_sessions) {
                let doc = doc! { "key": key, "data": data_doc };
                let opts = ReplaceOptions::builder().upsert(true).build();
                if let Err(e) = coll(&self.db, "sessions").replace_one(doc! { "key": key }, doc, opts).await {
                    tracing::error!(key = %key, error = %e, "failed to save session to mongo");
                }
            }
        }
    }

    /// Save a single session entry to MongoDB.
    pub async fn save_session(&self, key: &str, flow_sessions: &crate::api::FlowSessions) {
        if let Ok(data_doc) = to_doc(flow_sessions) {
            let doc = doc! { "key": key, "data": data_doc };
            let opts = ReplaceOptions::builder().upsert(true).build();
            if let Err(e) = coll(&self.db, "sessions").replace_one(doc! { "key": key }, doc, opts).await {
                tracing::error!(key = %key, error = %e, "failed to save session to mongo");
            }
        }
    }
}

fn to_doc<T: serde::Serialize>(val: &T) -> Result<Document> {
    let json = serde_json::to_value(val)?;
    let bson = mongodb::bson::to_bson(&json)?;
    match bson {
        mongodb::bson::Bson::Document(d) => Ok(d),
        _ => anyhow::bail!("expected BSON document"),
    }
}

fn from_doc<T: serde::de::DeserializeOwned>(doc: Document) -> Option<T> {
    let json: Value = mongodb::bson::from_document(doc).ok()?;
    serde_json::from_value(json).ok()
}

fn coll(db: &Database, name: &str) -> Collection<Document> {
    db.collection(name)
}

// ---------------------------------------------------------------------------
// AgentRepository
// ---------------------------------------------------------------------------

pub struct MongoAgentRepository {
    db: std::sync::Arc<MongoDb>,
}

impl MongoAgentRepository {
    pub fn new(db: std::sync::Arc<MongoDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AgentRepository for MongoAgentRepository {
    async fn list(&self) -> Vec<Agent> {
        let opts = FindOptions::builder().sort(doc! { "name": 1 }).build();
        let cursor = match coll(&self.db.db, "agents").find(doc! {}, opts).await {
            Ok(c) => c,
            Err(e) => { tracing::error!(error = %e, "mongo agent list"); return vec![]; }
        };
        let docs: Vec<Document> = cursor.try_collect().await.unwrap_or_default();
        docs.into_iter().filter_map(from_doc).collect()
    }

    async fn get(&self, id: &str) -> Option<Agent> {
        let doc = coll(&self.db.db, "agents").find_one(doc! { "id": id }, None).await.ok()??;
        from_doc(doc)
    }

    async fn save(&self, agent: Agent) -> Result<()> {
        let doc = to_doc(&agent)?;
        let opts = ReplaceOptions::builder().upsert(true).build();
        coll(&self.db.db, "agents").replace_one(doc! { "id": &agent.id }, doc, opts).await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let r = coll(&self.db.db, "agents").delete_one(doc! { "id": id }, None).await?;
        Ok(r.deleted_count > 0)
    }

    async fn load_all(&self) -> Result<()> { Ok(()) }
}

// ---------------------------------------------------------------------------
// FlowRepository
// ---------------------------------------------------------------------------

pub struct MongoFlowRepository {
    db: std::sync::Arc<MongoDb>,
}

impl MongoFlowRepository {
    pub fn new(db: std::sync::Arc<MongoDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl FlowRepository for MongoFlowRepository {
    async fn list_flows(&self) -> Vec<Flow> {
        let opts = FindOptions::builder().sort(doc! { "name": 1 }).build();
        let cursor = match coll(&self.db.db, "flows").find(doc! {}, opts).await {
            Ok(c) => c,
            Err(e) => { tracing::error!(error = %e, "mongo flow list"); return vec![]; }
        };
        let docs: Vec<Document> = cursor.try_collect().await.unwrap_or_default();
        docs.into_iter().filter_map(from_doc).collect()
    }

    async fn get_flow(&self, id: &str) -> Option<Flow> {
        let doc = coll(&self.db.db, "flows").find_one(doc! { "id": id }, None).await.ok()??;
        from_doc(doc)
    }

    async fn save_flow(&self, flow: Flow) -> Result<()> {
        let doc = to_doc(&flow)?;
        let opts = ReplaceOptions::builder().upsert(true).build();
        coll(&self.db.db, "flows").replace_one(doc! { "id": &flow.id }, doc, opts).await?;
        Ok(())
    }

    async fn delete_flow(&self, id: &str) -> Result<bool> {
        let r = coll(&self.db.db, "flows").delete_one(doc! { "id": id }, None).await?;
        coll(&self.db.db, "flow_runs").delete_many(doc! { "flow_id": id }, None).await.ok();
        Ok(r.deleted_count > 0)
    }

    async fn add_run(&self, run: FlowRun) -> Result<()> {
        let doc = to_doc(&run)?;
        coll(&self.db.db, "flow_runs").insert_one(doc, None).await?;
        Ok(())
    }

    async fn get_runs(&self, flow_id: &str, limit: usize) -> Vec<FlowRun> {
        let opts = FindOptions::builder().sort(doc! { "_id": -1 }).limit(limit as i64).build();
        let cursor = match coll(&self.db.db, "flow_runs").find(doc! { "flow_id": flow_id }, opts).await {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let docs: Vec<Document> = cursor.try_collect().await.unwrap_or_default();
        docs.into_iter().filter_map(from_doc).collect()
    }

    async fn complete_run(&self, flow_id: &str, run_id: &str, status: RunStatus, error: Option<String>) -> Result<()> {
        let status_str = serde_json::to_value(&status)?.as_str().unwrap_or("unknown").to_string();
        let finished = chrono::Utc::now().to_rfc3339();
        let mut set = doc! { "status": &status_str, "finished_at": &finished };
        if let Some(ref err) = error {
            set.insert("error", err.as_str());
        }
        coll(&self.db.db, "flow_runs")
            .update_one(doc! { "flow_id": flow_id, "id": run_id }, doc! { "$set": set }, None).await?;
        Ok(())
    }

    async fn push_node_run(&self, flow_id: &str, run_id: &str, node_run: NodeRun) -> Result<()> {
        let nr_doc = to_doc(&node_run)?;
        coll(&self.db.db, "flow_runs")
            .update_one(doc! { "flow_id": flow_id, "id": run_id }, doc! { "$push": { "node_runs": nr_doc } }, None).await?;
        Ok(())
    }

    async fn complete_node_run(&self, flow_id: &str, run_id: &str, node_id: &str, status: RunStatus, output_preview: Option<String>) -> Result<()> {
        let status_str = serde_json::to_value(&status)?.as_str().unwrap_or("unknown").to_string();
        let finished = chrono::Utc::now().to_rfc3339();
        let mut set = doc! { "node_runs.$.status": &status_str, "node_runs.$.finished_at": &finished };
        if let Some(ref preview) = output_preview {
            set.insert("node_runs.$.output_preview", preview.as_str());
        }
        coll(&self.db.db, "flow_runs")
            .update_one(doc! { "flow_id": flow_id, "id": run_id, "node_runs.node_id": node_id }, doc! { "$set": set }, None).await?;
        Ok(())
    }

    async fn load_all(&self) -> Result<()> { Ok(()) }
}

// ---------------------------------------------------------------------------
// PromptRepository
// ---------------------------------------------------------------------------

pub struct MongoPromptRepository {
    db: std::sync::Arc<MongoDb>,
}

impl MongoPromptRepository {
    pub fn new(db: std::sync::Arc<MongoDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl PromptRepository for MongoPromptRepository {
    async fn list_prompts(&self) -> Vec<SavedPrompt> {
        let opts = FindOptions::builder().sort(doc! { "title": 1 }).build();
        let cursor = match coll(&self.db.db, "prompts").find(doc! {}, opts).await {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let docs: Vec<Document> = cursor.try_collect().await.unwrap_or_default();
        docs.into_iter().filter_map(from_doc).collect()
    }

    async fn get_prompt(&self, id: &str) -> Option<SavedPrompt> {
        let doc = coll(&self.db.db, "prompts").find_one(doc! { "id": id }, None).await.ok()??;
        from_doc(doc)
    }

    async fn save_prompt(&self, prompt: SavedPrompt) -> Result<()> {
        let doc = to_doc(&prompt)?;
        let opts = ReplaceOptions::builder().upsert(true).build();
        coll(&self.db.db, "prompts").replace_one(doc! { "id": &prompt.id }, doc, opts).await?;
        Ok(())
    }

    async fn delete_prompt(&self, id: &str) -> Result<bool> {
        let r = coll(&self.db.db, "prompts").delete_one(doc! { "id": id }, None).await?;
        Ok(r.deleted_count > 0)
    }

    async fn load_all(&self) -> Result<()> { Ok(()) }
}

pub mod events;
pub mod file_repository;
pub mod graph;
pub mod history;
pub mod processors;
pub mod repository;
pub mod runner;
pub mod scheduler;
pub mod session_bridge;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub node_type: NodeType,
    pub kind: String,
    pub config: serde_json::Value,
    pub position: Position,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Trigger,
    Source,
    Executor,
    Sink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flow_roundtrip() {
        let flow = Flow {
            id: "test-id".to_string(),
            name: "Test Flow".to_string(),
            description: "A test flow".to_string(),
            enabled: true,
            nodes: vec![Node {
                id: "n1".to_string(),
                node_type: NodeType::Trigger,
                kind: "cron".to_string(),
                config: serde_json::json!({"schedule": "0 */4 * * *"}),
                position: Position { x: 0.0, y: 0.0 },
                label: "Every 4 hours".to_string(),
            }],
            edges: vec![],
            version: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&flow).unwrap();
        let parsed: Flow = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "test-id");
        assert_eq!(parsed.nodes.len(), 1);
        assert_eq!(parsed.nodes[0].node_type, NodeType::Trigger);
    }

    #[test]
    fn test_node_type_serialization() {
        assert_eq!(
            serde_json::to_string(&NodeType::Trigger).unwrap(),
            "\"trigger\""
        );
        assert_eq!(
            serde_json::to_string(&NodeType::Source).unwrap(),
            "\"source\""
        );
        assert_eq!(
            serde_json::to_string(&NodeType::Executor).unwrap(),
            "\"executor\""
        );
        assert_eq!(
            serde_json::to_string(&NodeType::Sink).unwrap(),
            "\"sink\""
        );
    }
}

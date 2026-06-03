use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An edge in the memory knowledge graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: Uuid,
    pub store_id: Uuid,
    pub tenant_id: String,
    pub source_memory_id: Uuid,
    pub target_memory_id: Uuid,
    pub relation_type: String,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

fn default_weight() -> f64 {
    1.0
}

/// Request to create a graph edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateEdgeRequest {
    pub source_memory_id: Uuid,
    pub target_memory_id: Uuid,
    pub relation_type: String,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Request for graph traversal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphTraversalRequest {
    pub start_memory_id: Uuid,
    #[serde(default = "default_depth")]
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_types: Option<Vec<String>>,
}

fn default_depth() -> u32 {
    2
}

/// Result of a graph traversal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTraversalResult {
    pub nodes: Vec<super::entry::MemoryEntry>,
    pub edges: Vec<GraphEdge>,
}

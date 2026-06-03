use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::scope::MemoryScope;

/// A single unit of stored memory within a store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: Uuid,
    pub store_id: Uuid,
    pub layer: MemoryLayer,
    pub content: serde_json::Value,
    /// Vector embedding (f32 array), computed by service or provided by caller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    pub owner_agent_id: String,
    pub scope: MemoryScope,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceReference>,
    pub access_control: AccessControl,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub immutable: bool,
    pub version: i64,
    // --- Temporal metadata (Level 3) ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    // --- Graph metadata (Level 3) ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    // --- Upsert support (see OMS spec 4.3.2) ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upsert_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLayer {
    Working,
    Episodic,
    Semantic,
    Procedural,
    Archival,
}

impl std::fmt::Display for MemoryLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Working => write!(f, "working"),
            Self::Episodic => write!(f, "episodic"),
            Self::Semantic => write!(f, "semantic"),
            Self::Procedural => write!(f, "procedural"),
            Self::Archival => write!(f, "archival"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessControl {
    pub policy: AccessPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_agents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_scopes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessPolicy {
    #[default]
    Private,
    Inherit,
    Shared,
    PublicRead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

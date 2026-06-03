use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::entry::{AccessControl, MemoryEntry, MemoryLayer, SourceReference};
use super::scope::MemoryScope;

/// A named, configured container for memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStore {
    pub id: Uuid,
    pub name: String,
    pub tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub config: StoreConfig,
    #[serde(default)]
    pub sovereignty: super::sovereignty::SovereigntyConfig,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Configuration for a memory store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoreConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_ttl_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sharing_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
}

/// Capabilities declared by a backend provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub supported_layers: Vec<MemoryLayer>,
    pub vector_search: bool,
    pub graph_support: bool,
    pub temporal_queries: bool,
    pub keyword_search: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_embedding_dimensions: Option<usize>,
    pub supported_distance_metrics: Vec<String>,
    pub compaction_support: bool,
    pub archival_support: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_entry_size_bytes: Option<usize>,
    pub batch_operations: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_batch_size: Option<usize>,
    pub pub_sub_notifications: bool,
    pub encryption_at_rest: bool,
    pub audit_log: bool,
}

/// Usage statistics for a store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreStats {
    pub store_id: Uuid,
    pub tenant_id: String,
    pub total_entries: u64,
    pub entries_by_layer: std::collections::HashMap<MemoryLayer, u64>,
    pub total_size_bytes: Option<u64>,
}

/// Input for creating a new store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStoreRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub config: StoreConfig,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

/// Input for updating a store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStoreRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<StoreConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// Input for creating a memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryRequest {
    #[serde(default = "default_layer")]
    pub layer: MemoryLayer,
    pub content: serde_json::Value,
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
    #[serde(default)]
    pub access_control: AccessControl,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub immutable: bool,
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
    /// When set, enables atomic create-or-replace within the same store, layer, and scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upsert_key: Option<String>,
}

fn default_layer() -> MemoryLayer {
    MemoryLayer::Working
}

/// Input for updating a memory entry.
/// Uses `Option<Option<T>>` for nullable fields so callers can distinguish
/// "don't change" (`None`) from "clear this field" (`Some(None)`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_double_option"
    )]
    pub embedding: Option<Option<Vec<f32>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<AccessControl>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_double_option"
    )]
    pub expires_at: Option<Option<DateTime<Utc>>>,
}

/// Deserialize a double-option field: absent → None, explicit null → Some(None), value → Some(Some(v)).
fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

/// Filter parameters for listing memories.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListMemoriesFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<MemoryLayer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Batch create request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCreateRequest {
    pub entries: Vec<CreateMemoryRequest>,
}

/// Batch create response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCreateResponse {
    pub created: Vec<MemoryEntry>,
    pub errors: Vec<BatchError>,
}

/// Batch delete request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDeleteRequest {
    pub memory_ids: Vec<Uuid>,
}

/// Batch delete response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDeleteResponse {
    pub deleted: u64,
    pub errors: Vec<BatchError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchError {
    pub index: usize,
    pub error: String,
}

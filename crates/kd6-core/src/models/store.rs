use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::entry::{AccessControl, MemoryEntry, MemoryLayer, SourceReference};
use super::scope::MemoryScope;

/// A named, configured container for memories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_ttl_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sharing_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
}

/// Capabilities declared by a backend provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreStats {
    pub store_id: Uuid,
    pub tenant_id: String,
    pub total_entries: u64,
    pub entries_by_layer: std::collections::HashMap<MemoryLayer, u64>,
    pub total_size_bytes: Option<u64>,
}

/// Input for creating a new store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStoreRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<StoreConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// Input for creating a memory entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AccessPolicy, Page};

    #[test]
    fn create_memory_defaults_to_working_layer() {
        let json = r#"{
            "content": "hello",
            "owner_agent_id": "agent-1",
            "scope": {"tenant_id": "t1"}
        }"#;
        let req: CreateMemoryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.layer, MemoryLayer::Working);
    }

    #[test]
    fn update_memory_double_option_absent_vs_null() {
        // Field absent → None (don't change)
        let json = r#"{}"#;
        let req: UpdateMemoryRequest = serde_json::from_str(json).unwrap();
        assert!(req.expires_at.is_none());

        // Field explicit null → Some(None) (clear field)
        let json = r#"{"expires_at": null}"#;
        let req: UpdateMemoryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.expires_at, Some(None));

        // Field with value → Some(Some(v))
        let json = r#"{"expires_at": "2025-01-01T00:00:00Z"}"#;
        let req: UpdateMemoryRequest = serde_json::from_str(json).unwrap();
        assert!(req.expires_at.unwrap().is_some());
    }

    #[test]
    fn memory_layer_display() {
        assert_eq!(MemoryLayer::Working.to_string(), "working");
        assert_eq!(MemoryLayer::Episodic.to_string(), "episodic");
        assert_eq!(MemoryLayer::Semantic.to_string(), "semantic");
        assert_eq!(MemoryLayer::Procedural.to_string(), "procedural");
        assert_eq!(MemoryLayer::Archival.to_string(), "archival");
    }

    #[test]
    fn memory_layer_serde_round_trip() {
        for layer in [
            MemoryLayer::Working,
            MemoryLayer::Episodic,
            MemoryLayer::Semantic,
            MemoryLayer::Procedural,
            MemoryLayer::Archival,
        ] {
            let json = serde_json::to_string(&layer).unwrap();
            let deserialized: MemoryLayer = serde_json::from_str(&json).unwrap();
            assert_eq!(layer, deserialized);
        }
    }

    #[test]
    fn access_policy_default_is_private() {
        assert_eq!(AccessPolicy::default(), AccessPolicy::Private);
    }

    #[test]
    fn access_policy_serde_snake_case() {
        let json = serde_json::to_string(&AccessPolicy::PublicRead).unwrap();
        assert_eq!(json, r#""public_read""#);
        let policy: AccessPolicy = serde_json::from_str(r#""shared""#).unwrap();
        assert_eq!(policy, AccessPolicy::Shared);
    }

    #[test]
    fn page_serde_round_trip() {
        let page = Page {
            items: vec!["a".to_string(), "b".to_string()],
            total: 10,
            limit: 2,
            offset: 0,
        };
        let json = serde_json::to_string(&page).unwrap();
        let deserialized: Page<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(page, deserialized);
    }

    #[test]
    fn store_config_defaults_to_none() {
        let config = StoreConfig::default();
        assert!(config.default_ttl_seconds.is_none());
        assert!(config.default_sharing_policy.is_none());
        assert!(config.embedding_model.is_none());
    }

    #[test]
    fn list_memories_filter_defaults() {
        let filter = ListMemoriesFilter::default();
        assert!(filter.layer.is_none());
        assert!(filter.tags.is_none());
        assert!(filter.limit.is_none());
        assert!(filter.offset.is_none());
    }

    #[test]
    fn batch_delete_request_serde() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let req = BatchDeleteRequest {
            memory_ids: vec![id1, id2],
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: BatchDeleteRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }
}

/// Filter parameters for listing memories.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchCreateRequest {
    pub entries: Vec<CreateMemoryRequest>,
}

/// Batch create response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchCreateResponse {
    pub created: Vec<MemoryEntry>,
    pub errors: Vec<BatchError>,
}

/// Batch delete request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchDeleteRequest {
    pub memory_ids: Vec<Uuid>,
}

/// Batch delete response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchDeleteResponse {
    pub deleted: u64,
    pub errors: Vec<BatchError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchError {
    pub index: usize,
    pub error: String,
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single audit log entry recording a write operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub store_id: Uuid,
    pub tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<Uuid>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    /// When true, this entry has been anonymized by GDPR purge.
    /// The `entry_hash` was computed from original (pre-redaction) content,
    /// so content-hash verification should be skipped, but chain verification
    /// (prev_hash links) remains valid.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redacted: bool,
}

/// Filter for querying audit logs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::entry::MemoryLayer;

/// Defines a parent-child memory inheritance relationship.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceSpec {
    pub id: Uuid,
    pub store_id: Uuid,
    pub tenant_id: String,
    pub parent_agent_id: String,
    pub child_agent_id: String,
    pub inherit_layers: Vec<MemoryLayer>,
    #[serde(default)]
    pub filter: InheritanceFilter,
    #[serde(default = "default_access")]
    pub access: InheritanceAccess,
    #[serde(default)]
    pub bubble_up: BubbleUpConfig,
    pub created_at: DateTime<Utc>,
}

fn default_access() -> InheritanceAccess {
    InheritanceAccess::ReadOnly
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InheritanceFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_entries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_from: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InheritanceAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BubbleUpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_summarize: bool,
    #[serde(default)]
    pub layers: Vec<MemoryLayer>,
}

/// Request to create an inheritance relationship.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInheritanceRequest {
    pub parent_agent_id: String,
    pub child_agent_id: String,
    pub inherit_layers: Vec<MemoryLayer>,
    #[serde(default)]
    pub filter: InheritanceFilter,
    #[serde(default = "default_access")]
    pub access: InheritanceAccess,
    #[serde(default)]
    pub bubble_up: BubbleUpConfig,
}

/// Request to bubble up child results to parent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BubbleUpRequest {
    pub child_agent_id: String,
    pub parent_agent_id: String,
    /// Optional summary to store as a new memory in the parent's scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<serde_json::Value>,
    /// Which layers to copy from child to parent.
    #[serde(default)]
    pub layers: Vec<MemoryLayer>,
}

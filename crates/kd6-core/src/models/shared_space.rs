use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::entry::MemoryLayer;
use super::scope::MemoryScope;

/// A shared memory space for peer agents (blackboard pattern).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedSpace {
    pub id: Uuid,
    pub name: String,
    pub store_id: Uuid,
    pub tenant_id: String,
    pub scope: MemoryScope,
    pub layer: MemoryLayer,
    pub conflict_resolution: ConflictResolution,
    pub notify_on_write: bool,
    pub notify_on_delete: bool,
    pub participants: Vec<SpaceParticipant>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    LastWriteWins,
    OrchestratorMerge,
    Crdt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceParticipant {
    pub agent_id: String,
    pub access: ParticipantAccess,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantAccess {
    ReadOnly,
    ReadWrite,
    Admin,
}

/// Request to create a shared memory space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSharedSpaceRequest {
    pub name: String,
    pub scope: MemoryScope,
    #[serde(default = "default_layer")]
    pub layer: MemoryLayer,
    #[serde(default = "default_conflict_resolution")]
    pub conflict_resolution: ConflictResolution,
    #[serde(default)]
    pub notify_on_write: bool,
    #[serde(default)]
    pub notify_on_delete: bool,
}

fn default_layer() -> MemoryLayer {
    MemoryLayer::Working
}

fn default_conflict_resolution() -> ConflictResolution {
    ConflictResolution::LastWriteWins
}

/// Request to join or add an agent to a shared space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinSpaceRequest {
    pub agent_id: String,
    #[serde(default = "default_participant_access")]
    pub access: ParticipantAccess,
}

fn default_participant_access() -> ParticipantAccess {
    ParticipantAccess::ReadWrite
}

/// Request to leave a shared space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveSpaceRequest {
    pub agent_id: String,
}

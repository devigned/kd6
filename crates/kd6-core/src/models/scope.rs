use serde::{Deserialize, Serialize};

/// Hierarchical scope controlling memory visibility.
///
/// A memory entry is visible to any agent whose scope is equal to or more
/// specific than the entry's scope. `tenant_id` is the hard isolation boundary.
///
/// **Important:** Providers must always override `scope.tenant_id` with the
/// authenticated `tenant_id` from the request context before storage.
/// Use [`MemoryScope::normalize`] to enforce this.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryScope {
    #[serde(default)]
    pub tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

impl MemoryScope {
    /// Override `tenant_id` with the authenticated value.
    pub fn normalize(mut self, tenant_id: &str) -> Self {
        self.tenant_id = tenant_id.to_string();
        self
    }
}

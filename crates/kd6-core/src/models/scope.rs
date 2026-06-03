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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_overrides_tenant_id() {
        let scope = MemoryScope {
            tenant_id: "user-provided".into(),
            org_id: Some("org-1".into()),
            ..Default::default()
        };
        let normalized = scope.normalize("auth-tenant");
        assert_eq!(normalized.tenant_id, "auth-tenant");
        assert_eq!(normalized.org_id, Some("org-1".into()));
    }

    #[test]
    fn normalize_sets_empty_tenant() {
        let scope = MemoryScope::default();
        assert_eq!(scope.tenant_id, "");
        let normalized = scope.normalize("t1");
        assert_eq!(normalized.tenant_id, "t1");
    }

    #[test]
    fn default_scope_has_all_none_fields() {
        let scope = MemoryScope::default();
        assert_eq!(scope.tenant_id, "");
        assert!(scope.org_id.is_none());
        assert!(scope.team_id.is_none());
        assert!(scope.project_id.is_none());
        assert!(scope.user_id.is_none());
        assert!(scope.agent_id.is_none());
        assert!(scope.session_id.is_none());
        assert!(scope.run_id.is_none());
    }

    #[test]
    fn scope_serde_round_trip() {
        let scope = MemoryScope {
            tenant_id: "t1".into(),
            org_id: Some("org".into()),
            team_id: None,
            project_id: Some("proj".into()),
            user_id: None,
            agent_id: Some("agent-1".into()),
            session_id: None,
            run_id: None,
        };
        let json = serde_json::to_string(&scope).unwrap();
        let deserialized: MemoryScope = serde_json::from_str(&json).unwrap();
        assert_eq!(scope, deserialized);
    }

    #[test]
    fn scope_omits_none_fields_in_json() {
        let scope = MemoryScope {
            tenant_id: "t1".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&scope).unwrap();
        assert!(!json.contains("org_id"));
        assert!(!json.contains("session_id"));
    }
}

use serde::{Deserialize, Serialize};

/// Data sovereignty configuration for a memory store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SovereigntyConfig {
    #[serde(default = "default_mode")]
    pub mode: SovereigntyMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub replication: ReplicationConfig,
}

impl Default for SovereigntyConfig {
    fn default() -> Self {
        Self {
            mode: SovereigntyMode::Any,
            region: None,
            replication: ReplicationConfig::default(),
        }
    }
}

fn default_mode() -> SovereigntyMode {
    SovereigntyMode::Any
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SovereigntyMode {
    Strict,
    Preferred,
    Any,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub target_regions: Vec<String>,
    #[serde(default = "default_consistency")]
    pub consistency: Consistency,
}

fn default_consistency() -> Consistency {
    Consistency::Eventual
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Consistency {
    Strong,
    #[default]
    Eventual,
}

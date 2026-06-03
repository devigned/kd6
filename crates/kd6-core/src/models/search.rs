use serde::{Deserialize, Serialize};

use super::entry::MemoryLayer;
use super::scope::MemoryScope;

/// Search query for vector similarity search with metadata filtering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Natural language query or structured query string.
    pub query: String,
    /// Pre-computed query embedding. If provided, used directly for similarity
    /// search. If absent, the service may compute one from `query`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Which layers to search (empty = all layers).
    #[serde(default)]
    pub layers: Vec<MemoryLayer>,
    /// Scope filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
    /// Maximum number of results.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Minimum similarity score (0.0–1.0).
    #[serde(default = "default_threshold")]
    pub threshold: f32,
    /// Additional metadata filters.
    #[serde(default)]
    pub filters: MetadataFilters,
    /// Include BM25/full-text keyword search results.
    #[serde(default)]
    pub keyword: bool,
}

fn default_top_k() -> usize {
    10
}

fn default_threshold() -> f32 {
    0.0
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_agent_id: Option<String>,
}

/// A single search result with similarity score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub entry: super::entry::MemoryEntry,
    pub score: f32,
}

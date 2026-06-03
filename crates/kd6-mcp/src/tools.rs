use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use kd6_core::embedding::{auto_embed_content, auto_embed_query, EmbeddingProvider};
use kd6_core::models::{
    AccessControl, CreateEdgeRequest, CreateMemoryRequest, CreateStoreRequest,
    GraphTraversalRequest, MemoryLayer, MemoryScope, SearchQuery, StoreConfig,
};
use kd6_core::OmsProvider;

/// KD6 MCP Server — exposes OMS memory operations as MCP tools.
#[derive(Clone)]
pub struct Kd6McpServer {
    provider: Arc<dyn OmsProvider>,
    embedder: Arc<dyn EmbeddingProvider>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl Kd6McpServer {
    pub fn new(provider: Arc<dyn OmsProvider>, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            provider,
            embedder,
            tool_router: Self::tool_router(),
        }
    }

    /// Return the list of registered MCP tools (useful for testing/introspection).
    pub fn list_tools(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }
}

#[tool_handler(
    name = "kd6",
    version = "0.1.0",
    instructions = "KD6 is an Open Memory Service for agentic AI. Use these tools to create stores, save memories, search, and manage knowledge graphs."
)]
impl ServerHandler for Kd6McpServer {}

// --- Parameter types ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateStoreParams {
    /// Tenant identifier for isolation.
    pub tenant_id: String,
    /// Human-readable store name.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListStoresParams {
    /// Tenant identifier.
    pub tenant_id: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CreateMemoryParams {
    /// Tenant identifier.
    #[serde(default)]
    pub tenant_id: String,
    /// Store ID to create the memory in.
    #[serde(default)]
    pub store_id: String,
    /// Memory layer: working, episodic, semantic, procedural, or archival.
    #[serde(default = "default_layer_str")]
    pub layer: String,
    /// The memory content as a JSON string or plain text.
    #[serde(default)]
    pub content: serde_json::Value,
    /// Agent that owns this memory.
    #[serde(default)]
    pub owner_agent_id: String,
    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Optional entity type for graph nodes.
    #[serde(default)]
    pub entity_type: Option<String>,
    /// Optional upsert key for atomic create-or-replace (see OMS spec 4.3.2).
    #[serde(default)]
    pub upsert_key: Option<String>,
    /// Optional scope fields for finer-grained visibility.
    #[serde(default)]
    pub scope_user_id: Option<String>,
    #[serde(default)]
    pub scope_agent_id: Option<String>,
    #[serde(default)]
    pub scope_session_id: Option<String>,
}

fn default_layer_str() -> String {
    "working".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetMemoryParams {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Store ID.
    pub store_id: String,
    /// Memory entry ID.
    pub memory_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchMemoriesParams {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Store ID to search within.
    pub store_id: String,
    /// Natural language search query.
    pub query: String,
    /// Maximum number of results (default: 10).
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Include keyword (BM25) search results (default: true, since MCP has no embedding support).
    #[serde(default = "default_keyword")]
    pub keyword: bool,
}

fn default_top_k() -> usize {
    10
}

fn default_keyword() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteMemoryParams {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Store ID.
    pub store_id: String,
    /// Memory entry ID to delete.
    pub memory_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateEdgeParams {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Store ID.
    pub store_id: String,
    /// Source memory ID.
    pub source_memory_id: String,
    /// Target memory ID.
    pub target_memory_id: String,
    /// Relationship type (e.g., "related_to", "depends_on", "part_of").
    pub relation_type: String,
    /// Edge weight (default: 1.0).
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphTraverseParams {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Store ID.
    pub store_id: String,
    /// Starting memory ID for traversal.
    pub start_memory_id: String,
    /// Traversal depth (default: 2).
    #[serde(default = "default_depth")]
    pub depth: u32,
    /// Filter by relation types (optional).
    #[serde(default)]
    pub relation_types: Option<Vec<String>>,
}

fn default_depth() -> u32 {
    2
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StoreStatsParams {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Store ID.
    pub store_id: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GdprPurgeParams {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Store ID.
    pub store_id: String,
    /// Scope to purge. All memories matching this scope will be permanently deleted
    /// and associated audit entries will be anonymized.
    #[serde(default)]
    pub scope_org_id: Option<String>,
    #[serde(default)]
    pub scope_team_id: Option<String>,
    #[serde(default)]
    pub scope_project_id: Option<String>,
    #[serde(default)]
    pub scope_user_id: Option<String>,
    #[serde(default)]
    pub scope_agent_id: Option<String>,
    #[serde(default)]
    pub scope_session_id: Option<String>,
    #[serde(default)]
    pub scope_run_id: Option<String>,
}

// --- Tool result helper ---

#[derive(Serialize)]
struct ToolResult<T: Serialize> {
    success: bool,
    data: T,
}

#[derive(Serialize)]
struct ToolError {
    success: bool,
    error: String,
}

fn ok_json<T: Serialize>(data: T) -> String {
    serde_json::to_string_pretty(&ToolResult {
        success: true,
        data,
    })
    .unwrap_or_else(|_| r#"{"success":false,"error":"internal serialization error"}"#.to_string())
}

fn err_json(msg: String) -> String {
    serde_json::to_string(&ToolError {
        success: false,
        error: msg,
    })
    .unwrap_or_else(|_| r#"{"success":false,"error":"internal serialization error"}"#.to_string())
}

fn parse_uuid(s: &str) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(s).map_err(|e| format!("invalid UUID '{s}': {e}"))
}

fn validate_tenant_id(tenant_id: &str) -> Result<(), String> {
    if tenant_id.trim().is_empty() {
        return Err("tenant_id is required and cannot be empty".to_string());
    }
    Ok(())
}

fn parse_layer(s: &str) -> Result<MemoryLayer, String> {
    match s {
        "working" => Ok(MemoryLayer::Working),
        "episodic" => Ok(MemoryLayer::Episodic),
        "semantic" => Ok(MemoryLayer::Semantic),
        "procedural" => Ok(MemoryLayer::Procedural),
        "archival" => Ok(MemoryLayer::Archival),
        other => Err(format!(
            "unknown layer '{other}': must be one of working, episodic, semantic, procedural, archival"
        )),
    }
}

// --- Tool implementations ---

#[tool_router]
impl Kd6McpServer {
    #[tool(description = "Create a new memory store for organizing memories by tenant")]
    pub async fn create_store(&self, Parameters(p): Parameters<CreateStoreParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        match self
            .provider
            .create_store(
                &p.tenant_id,
                CreateStoreRequest {
                    name: p.name,
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
        {
            Ok(store) => ok_json(store),
            Err(e) => err_json(e.to_string()),
        }
    }

    #[tool(description = "List all memory stores for a tenant")]
    pub async fn list_stores(&self, Parameters(p): Parameters<ListStoresParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        match self.provider.list_stores(&p.tenant_id).await {
            Ok(stores) => ok_json(stores),
            Err(e) => err_json(e.to_string()),
        }
    }

    #[tool(description = "Create a new memory entry in a store")]
    pub async fn create_memory(&self, Parameters(p): Parameters<CreateMemoryParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        let store_id = match parse_uuid(&p.store_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };

        let layer = match parse_layer(&p.layer) {
            Ok(l) => l,
            Err(e) => return err_json(e),
        };

        let content: serde_json::Value =
            if p.content.is_null() || p.content == serde_json::Value::String(String::new()) {
                serde_json::Value::String(String::new())
            } else {
                p.content
            };

        let request = CreateMemoryRequest {
            layer,
            content,
            embedding: None,
            owner_agent_id: p.owner_agent_id,
            scope: MemoryScope {
                tenant_id: p.tenant_id.clone(),
                user_id: p.scope_user_id,
                agent_id: p.scope_agent_id,
                session_id: p.scope_session_id,
                ..Default::default()
            },
            tags: p.tags,
            categories: vec![],
            source: None,
            access_control: AccessControl::default(),
            expires_at: None,
            immutable: false,
            valid_from: None,
            valid_until: None,
            confidence: None,
            entity_type: p.entity_type,
            upsert_key: p.upsert_key,
        };

        // Auto-embed content (OMS spec section 8.4.1)
        let embedding =
            match auto_embed_content(&*self.embedder, &request.content, request.embedding.clone())
                .await
            {
                Ok(emb) => emb,
                Err(e) => return err_json(e.to_string()),
            };

        let request = CreateMemoryRequest {
            embedding,
            ..request
        };

        match self
            .provider
            .create_memory(&p.tenant_id, store_id, request)
            .await
        {
            Ok(entry) => ok_json(entry),
            Err(e) => err_json(e.to_string()),
        }
    }

    #[tool(description = "Retrieve a specific memory entry by ID")]
    pub async fn get_memory(&self, Parameters(p): Parameters<GetMemoryParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        let store_id = match parse_uuid(&p.store_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };
        let memory_id = match parse_uuid(&p.memory_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };

        match self
            .provider
            .get_memory(&p.tenant_id, store_id, memory_id)
            .await
        {
            Ok(entry) => ok_json(entry),
            Err(e) => err_json(e.to_string()),
        }
    }

    #[tool(description = "Search memories using natural language or keyword queries")]
    pub async fn search_memories(&self, Parameters(p): Parameters<SearchMemoriesParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        let store_id = match parse_uuid(&p.store_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };

        // Auto-embed query (OMS spec section 8.4.3)
        let embedding = match auto_embed_query(&*self.embedder, &p.query, None).await {
            Ok(emb) => emb,
            Err(e) => return err_json(e.to_string()),
        };

        let query = SearchQuery {
            query: p.query,
            embedding,
            layers: vec![],
            scope: None,
            top_k: p.top_k,
            threshold: 0.0,
            filters: Default::default(),
            keyword: p.keyword,
        };

        match self.provider.search(&p.tenant_id, store_id, query).await {
            Ok(results) => ok_json(results),
            Err(e) => err_json(e.to_string()),
        }
    }

    #[tool(description = "Delete a memory entry by ID")]
    pub async fn delete_memory(&self, Parameters(p): Parameters<DeleteMemoryParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        let store_id = match parse_uuid(&p.store_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };
        let memory_id = match parse_uuid(&p.memory_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };

        match self
            .provider
            .delete_memory(&p.tenant_id, store_id, memory_id)
            .await
        {
            Ok(()) => ok_json("memory deleted"),
            Err(e) => err_json(e.to_string()),
        }
    }

    #[tool(description = "Create a relationship edge between two memories in the knowledge graph")]
    pub async fn create_edge(&self, Parameters(p): Parameters<CreateEdgeParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        let store_id = match parse_uuid(&p.store_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };
        let source = match parse_uuid(&p.source_memory_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };
        let target = match parse_uuid(&p.target_memory_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };

        let request = CreateEdgeRequest {
            source_memory_id: source,
            target_memory_id: target,
            relation_type: p.relation_type,
            weight: p.weight,
            metadata: serde_json::json!({}),
        };

        match self
            .provider
            .create_edge(&p.tenant_id, store_id, request)
            .await
        {
            Ok(edge) => ok_json(edge),
            Err(e) => err_json(e.to_string()),
        }
    }

    #[tool(
        name = "traverse_graph",
        description = "Traverse the knowledge graph starting from a memory, following relationship edges"
    )]
    pub async fn traverse_graph(&self, Parameters(p): Parameters<GraphTraverseParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        let store_id = match parse_uuid(&p.store_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };
        let start_id = match parse_uuid(&p.start_memory_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };

        let request = GraphTraversalRequest {
            start_memory_id: start_id,
            depth: p.depth,
            relation_types: p.relation_types,
        };

        match self
            .provider
            .graph_traverse(&p.tenant_id, store_id, request)
            .await
        {
            Ok(result) => ok_json(result),
            Err(e) => err_json(e.to_string()),
        }
    }

    #[tool(description = "Get statistics for a memory store")]
    pub async fn store_stats(&self, Parameters(p): Parameters<StoreStatsParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        let store_id = match parse_uuid(&p.store_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };

        match self.provider.stats(&p.tenant_id, store_id).await {
            Ok(stats) => ok_json(stats),
            Err(e) => err_json(e.to_string()),
        }
    }

    #[tool(
        description = "GDPR purge: permanently delete all memories matching the given scope and anonymize related audit entries"
    )]
    pub async fn gdpr_purge(&self, Parameters(p): Parameters<GdprPurgeParams>) -> String {
        if let Err(e) = validate_tenant_id(&p.tenant_id) {
            return err_json(e);
        }
        let store_id = match parse_uuid(&p.store_id) {
            Ok(id) => id,
            Err(e) => return err_json(e),
        };

        let scope = MemoryScope {
            tenant_id: p.tenant_id.clone(),
            org_id: p.scope_org_id,
            team_id: p.scope_team_id,
            project_id: p.scope_project_id,
            user_id: p.scope_user_id,
            agent_id: p.scope_agent_id,
            session_id: p.scope_session_id,
            run_id: p.scope_run_id,
        };

        match self
            .provider
            .gdpr_purge(&p.tenant_id, store_id, scope)
            .await
        {
            Ok(deleted) => ok_json(serde_json::json!({ "deleted": deleted })),
            Err(e) => err_json(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn parse_uuid_accepts_valid_uuid() {
        let uuid = Uuid::new_v4();

        assert_eq!(parse_uuid(&uuid.to_string()).unwrap(), uuid);
    }

    #[test]
    fn parse_uuid_rejects_invalid_uuid() {
        let error = parse_uuid("not-a-uuid").unwrap_err();

        assert!(error.contains("invalid UUID 'not-a-uuid'"));
    }

    #[test]
    fn parse_layer_accepts_all_valid_layers() {
        let cases = [
            ("working", MemoryLayer::Working),
            ("episodic", MemoryLayer::Episodic),
            ("semantic", MemoryLayer::Semantic),
            ("procedural", MemoryLayer::Procedural),
            ("archival", MemoryLayer::Archival),
        ];

        for (input, expected) in cases {
            assert_eq!(parse_layer(input).unwrap(), expected);
        }
    }

    #[test]
    fn parse_layer_rejects_invalid_layer() {
        let error = parse_layer("invalid-layer").unwrap_err();

        assert!(error.contains("unknown layer 'invalid-layer'"));
    }

    #[test]
    fn validate_tenant_id_rejects_empty_and_whitespace() {
        assert!(validate_tenant_id("tenant-1").is_ok());
        assert_eq!(
            validate_tenant_id("").unwrap_err(),
            "tenant_id is required and cannot be empty"
        );
        assert_eq!(
            validate_tenant_id("   ").unwrap_err(),
            "tenant_id is required and cannot be empty"
        );
    }

    #[test]
    fn ok_json_serializes_successful_payload() {
        let value: serde_json::Value =
            serde_json::from_str(&ok_json(json!({"name": "store"}))).unwrap();

        assert_eq!(value["success"], json!(true));
        assert_eq!(value["data"], json!({"name": "store"}));
    }

    #[test]
    fn err_json_serializes_error_payload() {
        let value: serde_json::Value = serde_json::from_str(&err_json("boom".to_string())).unwrap();

        assert_eq!(value["success"], json!(false));
        assert_eq!(value["error"], json!("boom"));
    }
}

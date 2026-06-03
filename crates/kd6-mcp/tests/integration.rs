use std::sync::Arc;

use kd6_core::{EmbeddingProvider, NoopEmbedder, OmsProvider};
use kd6_mcp::{
    CreateEdgeParams, CreateMemoryParams, CreateStoreParams, DeleteMemoryParams, GdprPurgeParams,
    GetMemoryParams, GraphTraverseParams, Kd6McpServer, ListStoresParams, SearchMemoriesParams,
    StoreStatsParams,
};
use kd6_sqlite::SqliteProvider;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::ServerHandler;
use serde_json::{json, Value};

const TENANT_ID: &str = "tenant-1";

async fn test_server() -> Kd6McpServer {
    let provider = SqliteProvider::new("sqlite::memory:").await.unwrap();
    Kd6McpServer::new(
        Arc::new(provider) as Arc<dyn OmsProvider>,
        Arc::new(NoopEmbedder),
    )
}

fn parse_response(response: String) -> Value {
    serde_json::from_str(&response).unwrap()
}

async fn create_store(server: &Kd6McpServer, tenant_id: &str, name: &str) -> Value {
    parse_response(
        server
            .create_store(Parameters(CreateStoreParams {
                tenant_id: tenant_id.to_string(),
                name: name.to_string(),
            }))
            .await,
    )
}

async fn create_memory(
    server: &Kd6McpServer,
    tenant_id: &str,
    store_name: &str,
    content: &str,
) -> Value {
    parse_response(
        server
            .create_memory(Parameters(CreateMemoryParams {
                tenant_id: tenant_id.to_string(),
                store_name: store_name.to_string(),
                layer: "working".to_string(),
                content: json!(content),
                owner_agent_id: "agent-1".to_string(),
                tags: vec!["test".to_string()],
                ..Default::default()
            }))
            .await,
    )
}

#[tokio::test]
async fn create_store_returns_created_store() {
    let server = test_server().await;

    let response = create_store(&server, TENANT_ID, "test-store").await;

    assert_eq!(response["success"], json!(true));
    assert_eq!(response["data"]["name"], json!("test-store"));
    assert_eq!(response["data"]["tenant_id"], json!(TENANT_ID));
}

#[tokio::test]
async fn list_stores_reflects_created_store() {
    let server = test_server().await;

    let empty = parse_response(
        server
            .list_stores(Parameters(ListStoresParams {
                tenant_id: TENANT_ID.to_string(),
            }))
            .await,
    );
    assert_eq!(empty["success"], json!(true));
    assert_eq!(empty["data"], json!([]));

    create_store(&server, TENANT_ID, "test-store").await;

    let listed = parse_response(
        server
            .list_stores(Parameters(ListStoresParams {
                tenant_id: TENANT_ID.to_string(),
            }))
            .await,
    );
    assert_eq!(listed["success"], json!(true));
    assert_eq!(listed["data"].as_array().unwrap().len(), 1);
    assert_eq!(listed["data"][0]["name"], json!("test-store"));
}

#[tokio::test]
async fn create_memory_returns_created_entry() {
    let server = test_server().await;
    let store = create_store(&server, TENANT_ID, "test-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();

    let response = create_memory(&server, TENANT_ID, store_name, "remember this").await;

    assert_eq!(response["success"], json!(true));
    assert!(response["data"]["store_id"].is_string());
    assert_eq!(response["data"]["content"], json!("remember this"));
}

#[tokio::test]
async fn get_memory_returns_created_entry() {
    let server = test_server().await;
    let store = create_store(&server, TENANT_ID, "test-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();
    let created = create_memory(&server, TENANT_ID, store_name, "remember me").await;
    let memory_id = created["data"]["id"].as_str().unwrap();

    let fetched = parse_response(
        server
            .get_memory(Parameters(GetMemoryParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
                memory_id: memory_id.to_string(),
            }))
            .await,
    );

    assert_eq!(fetched["success"], json!(true));
    assert_eq!(fetched["data"]["id"], json!(memory_id));
    assert_eq!(fetched["data"]["content"], json!("remember me"));
}

#[tokio::test]
async fn search_memories_finds_keyword_match() {
    let server = test_server().await;
    let store = create_store(&server, TENANT_ID, "test-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();
    create_memory(&server, TENANT_ID, store_name, "alpha keyword match").await;

    let response = parse_response(
        server
            .search_memories(Parameters(SearchMemoriesParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
                query: "alpha".to_string(),
                top_k: 10,
                keyword: true,
            }))
            .await,
    );

    assert_eq!(response["success"], json!(true));
    assert_eq!(response["data"].as_array().unwrap().len(), 1);
    assert_eq!(
        response["data"][0]["entry"]["content"],
        json!("alpha keyword match")
    );
}

#[tokio::test]
async fn delete_memory_removes_entry() {
    let server = test_server().await;
    let store = create_store(&server, TENANT_ID, "test-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();
    let created = create_memory(&server, TENANT_ID, store_name, "delete me").await;
    let memory_id = created["data"]["id"].as_str().unwrap();

    let deleted = parse_response(
        server
            .delete_memory(Parameters(DeleteMemoryParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
                memory_id: memory_id.to_string(),
            }))
            .await,
    );
    assert_eq!(deleted["success"], json!(true));
    assert_eq!(deleted["data"], json!("memory deleted"));

    let fetched = parse_response(
        server
            .get_memory(Parameters(GetMemoryParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
                memory_id: memory_id.to_string(),
            }))
            .await,
    );
    assert_eq!(fetched["success"], json!(false));
    assert!(fetched["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn create_edge_returns_created_relationship() {
    let server = test_server().await;
    let store = create_store(&server, TENANT_ID, "test-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();
    let source = create_memory(&server, TENANT_ID, store_name, "Node A").await;
    let target = create_memory(&server, TENANT_ID, store_name, "Node B").await;
    let source_id = source["data"]["id"].as_str().unwrap();
    let target_id = target["data"]["id"].as_str().unwrap();

    let response = parse_response(
        server
            .create_edge(Parameters(CreateEdgeParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
                source_memory_id: source_id.to_string(),
                target_memory_id: target_id.to_string(),
                relation_type: "related_to".to_string(),
                weight: 1.0,
            }))
            .await,
    );

    assert_eq!(response["success"], json!(true));
    assert_eq!(response["data"]["source_memory_id"], json!(source_id));
    assert_eq!(response["data"]["target_memory_id"], json!(target_id));
    assert_eq!(response["data"]["relation_type"], json!("related_to"));
}

#[tokio::test]
async fn graph_traverse_returns_neighboring_nodes() {
    let server = test_server().await;
    let store = create_store(&server, TENANT_ID, "test-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();
    let source = create_memory(&server, TENANT_ID, store_name, "Node A").await;
    let target = create_memory(&server, TENANT_ID, store_name, "Node B").await;
    let source_id = source["data"]["id"].as_str().unwrap();
    let target_id = target["data"]["id"].as_str().unwrap();

    parse_response(
        server
            .create_edge(Parameters(CreateEdgeParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
                source_memory_id: source_id.to_string(),
                target_memory_id: target_id.to_string(),
                relation_type: "related_to".to_string(),
                weight: 1.0,
            }))
            .await,
    );

    let response = parse_response(
        server
            .traverse_graph(Parameters(GraphTraverseParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
                start_memory_id: source_id.to_string(),
                depth: 1,
                relation_types: None,
            }))
            .await,
    );

    assert_eq!(response["success"], json!(true));
    assert_eq!(response["data"]["edges"].as_array().unwrap().len(), 1);
    assert!(response["data"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["id"] == json!(target_id)));
}

#[tokio::test]
async fn store_stats_returns_store_statistics() {
    let server = test_server().await;
    let store = create_store(&server, TENANT_ID, "test-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();

    let response = parse_response(
        server
            .store_stats(Parameters(StoreStatsParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
            }))
            .await,
    );

    assert_eq!(response["success"], json!(true));
    assert!(response["data"]["store_id"].is_string());
    assert_eq!(response["data"]["tenant_id"], json!(TENANT_ID));
    assert_eq!(response["data"]["total_entries"], json!(0));
}

#[tokio::test]
async fn create_store_rejects_empty_tenant_id() {
    let server = test_server().await;

    let response = create_store(&server, "", "test-store").await;

    assert_eq!(response["success"], json!(false));
    assert_eq!(
        response["error"],
        json!("tenant_id is required and cannot be empty")
    );
}

#[tokio::test]
async fn create_memory_rejects_nonexistent_store() {
    let server = test_server().await;

    let response = create_memory(&server, TENANT_ID, "nonexistent-store", "bad store").await;

    assert_eq!(response["success"], json!(false));
    assert!(response["error"].as_str().unwrap().contains("not found"));
}

// ---------------------------------------------------------------------------
// Embedding-aware MCP tests
// ---------------------------------------------------------------------------

/// Deterministic fake embedder for tests (avoids slow model download).
/// Produces 3-dimensional vectors based on text length.
struct FakeEmbedder;

#[async_trait::async_trait]
impl EmbeddingProvider for FakeEmbedder {
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, kd6_core::OmsError> {
        Ok(texts
            .iter()
            .map(|t| {
                let len = t.len() as f32;
                vec![len, len * 0.5, 1.0]
            })
            .collect())
    }
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>, kd6_core::OmsError> {
        let len = query.len() as f32;
        Ok(vec![len, len * 0.5, 1.0])
    }
    fn dimensions(&self) -> usize {
        3
    }
    fn model_id(&self) -> &str {
        "fake-3d"
    }
}

async fn test_server_with_embedder() -> Kd6McpServer {
    let provider = SqliteProvider::new("sqlite::memory:").await.unwrap();
    Kd6McpServer::new(
        Arc::new(provider) as Arc<dyn OmsProvider>,
        Arc::new(FakeEmbedder) as Arc<dyn EmbeddingProvider>,
    )
}

#[tokio::test]
async fn mcp_create_memory_produces_embedding() {
    let server = test_server_with_embedder().await;
    let store = create_store(&server, TENANT_ID, "embed-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();

    let response = create_memory(
        &server,
        TENANT_ID,
        store_name,
        "embeddings should be auto-computed",
    )
    .await;

    assert_eq!(response["success"], json!(true));
    assert!(
        response["data"]["embedding"].is_array(),
        "MCP create_memory should auto-embed when embedder is configured"
    );
    let dims = response["data"]["embedding"].as_array().unwrap().len();
    assert_eq!(dims, 3, "FakeEmbedder produces 3-dim vectors");
}

#[tokio::test]
async fn mcp_vector_search_returns_results() {
    let server = test_server_with_embedder().await;
    let store = create_store(&server, TENANT_ID, "search-embed-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();

    // Add documents with varying content lengths for distinct embeddings
    for text in [
        "short",
        "a medium length document about many topics",
        "another quite different and longer document for testing purposes here",
    ] {
        create_memory(&server, TENANT_ID, store_name, text).await;
    }

    // Vector search
    let response = parse_response(
        server
            .search_memories(Parameters(SearchMemoriesParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
                query: "short".to_string(),
                top_k: 3,
                keyword: false,
            }))
            .await,
    );

    assert_eq!(response["success"], json!(true));
    let results = response["data"].as_array().unwrap();
    assert!(
        !results.is_empty(),
        "vector search should return results with FakeEmbedder"
    );
}

#[tokio::test]
async fn mcp_noop_embedder_allows_keyword_search() {
    // With NoopEmbedder, keyword search should still work
    let server = test_server().await;
    let store = create_store(&server, TENANT_ID, "keyword-only-store").await;
    let store_name = store["data"]["name"].as_str().unwrap();

    create_memory(&server, TENANT_ID, store_name, "unique keyword testphrase").await;

    let response = parse_response(
        server
            .search_memories(Parameters(SearchMemoriesParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.to_string(),
                query: "testphrase".to_string(),
                top_k: 10,
                keyword: true,
            }))
            .await,
    );

    assert_eq!(response["success"], json!(true));
    let results = response["data"].as_array().unwrap();
    assert_eq!(results.len(), 1, "keyword search should find the document");
}

#[tokio::test]
async fn mcp_gdpr_purge_deletes_scoped_memories() {
    let server = test_server().await;

    // Create a store
    let store_response = parse_response(
        server
            .create_store(Parameters(CreateStoreParams {
                tenant_id: TENANT_ID.to_string(),
                name: "gdpr-test".to_string(),
            }))
            .await,
    );
    let store_name = store_response["data"]["name"].as_str().unwrap().to_string();

    // Create two memories with different scopes
    let _ = parse_response(
        server
            .create_memory(Parameters(CreateMemoryParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.clone(),
                content: json!("user-a data"),
                layer: "working".to_string(),
                owner_agent_id: "test-agent".to_string(),
                scope_user_id: Some("user-a".to_string()),
                ..Default::default()
            }))
            .await,
    );
    let _ = parse_response(
        server
            .create_memory(Parameters(CreateMemoryParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.clone(),
                content: json!("user-b data"),
                layer: "working".to_string(),
                owner_agent_id: "test-agent".to_string(),
                scope_user_id: Some("user-b".to_string()),
                ..Default::default()
            }))
            .await,
    );

    // Purge user-a
    let purge_response = parse_response(
        server
            .gdpr_purge(Parameters(GdprPurgeParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.clone(),
                scope_user_id: Some("user-a".to_string()),
                ..Default::default()
            }))
            .await,
    );
    assert_eq!(purge_response["success"], json!(true));
    assert_eq!(purge_response["data"]["deleted"], json!(1));

    // Verify user-b's data still exists via search
    let search_response = parse_response(
        server
            .search_memories(Parameters(SearchMemoriesParams {
                tenant_id: TENANT_ID.to_string(),
                store_name: store_name.clone(),
                query: "user-b".to_string(),
                top_k: 10,
                keyword: true,
            }))
            .await,
    );
    assert_eq!(search_response["success"], json!(true));
    let results = search_response["data"].as_array().unwrap();
    assert_eq!(results.len(), 1, "user-b data should survive purge");
}

// ---------------------------------------------------------------------------
// MCP transport smoke tests
// ---------------------------------------------------------------------------

/// Verify the server registers exactly the expected 10 tools.
#[tokio::test]
async fn test_mcp_tool_registration() {
    let server = test_server().await;
    let tools = server.list_tools();
    let tool_names: std::collections::BTreeSet<String> =
        tools.iter().map(|t| t.name.to_string()).collect();

    let expected: std::collections::BTreeSet<String> = [
        "create_store",
        "list_stores",
        "store_stats",
        "create_memory",
        "get_memory",
        "search_memories",
        "delete_memory",
        "create_edge",
        "traverse_graph",
        "gdpr_purge",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    assert_eq!(
        tool_names, expected,
        "registered tools mismatch: got {tool_names:?}"
    );
}

/// Verify server metadata (name, version) is set correctly.
#[tokio::test]
async fn test_mcp_server_info() {
    let server = test_server().await;
    let info = server.get_info();

    assert_eq!(info.server_info.name, "kd6");
    assert_eq!(info.server_info.version, "0.1.0");
    assert!(
        info.instructions.is_some(),
        "server should have instructions"
    );
}

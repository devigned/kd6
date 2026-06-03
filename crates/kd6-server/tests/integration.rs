use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum_test::TestServer;
use serde_json::{json, Value};

use kd6_core::NoopEmbedder;
use kd6_server::state::{AppState, ServerConfig};
use kd6_sqlite::SqliteProvider;

async fn test_app() -> TestServer {
    let provider = SqliteProvider::new("sqlite::memory:").await.unwrap();
    let state = AppState {
        provider: Arc::new(provider),
        embedder: Arc::new(NoopEmbedder),
        config: ServerConfig::default(),
    };

    let app = kd6_server::build_router(state);
    TestServer::new(app).unwrap()
}

/// Test app with default tenant and auto-provisioning disabled (strict mode).
async fn test_app_strict() -> TestServer {
    let provider = SqliteProvider::new("sqlite::memory:").await.unwrap();
    let state = AppState {
        provider: Arc::new(provider),
        embedder: Arc::new(NoopEmbedder),
        config: ServerConfig {
            auto_provision: false,
            default_tenant: false,
        },
    };

    let app = kd6_server::build_router(state);
    TestServer::new(app).unwrap()
}

fn tenant_header(request: axum_test::TestRequest, tenant: &str) -> axum_test::TestRequest {
    request.add_header(
        HeaderName::from_static("x-tenant-id"),
        HeaderValue::from_str(tenant).unwrap(),
    )
}

#[tokio::test]
async fn test_health_endpoint() {
    let server = test_app().await;
    let response = server.get("/health").await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn test_missing_tenant_header_returns_401_json() {
    let server = test_app_strict().await;
    let response = server.get("/v1/stores").await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    let body: Value = response.json();
    assert!(body["error"].as_str().unwrap().contains("X-Tenant-ID"));
}

#[tokio::test]
async fn test_empty_tenant_header_returns_401() {
    let server = test_app_strict().await;
    let response = tenant_header(server.get("/v1/stores"), "").await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    let body: Value = response.json();
    assert!(body["error"].as_str().unwrap().contains("X-Tenant-ID"));
}

#[tokio::test]
async fn test_whitespace_tenant_header_returns_401() {
    let server = test_app_strict().await;
    let response = tenant_header(server.get("/v1/stores"), "   ").await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    let body: Value = response.json();
    assert!(body["error"].as_str().unwrap().contains("X-Tenant-ID"));
}

#[tokio::test]
async fn test_create_get_list_update_and_delete_store() {
    let server = test_app().await;
    let tenant = "test-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "my-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    assert_eq!(store["name"], "my-store");
    assert_eq!(store["tenant_id"], tenant);
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(server.get(&format!("/v1/stores/{store_id}")), tenant).await;
    response.assert_status_ok();
    let fetched: Value = response.json();
    assert_eq!(fetched["name"], "my-store");

    let response = tenant_header(server.get("/v1/stores"), tenant).await;
    response.assert_status_ok();
    let stores: Vec<Value> = response.json();
    assert_eq!(stores.len(), 1);

    let response = tenant_header(server.patch(&format!("/v1/stores/{store_id}")), tenant)
        .json(&json!({ "name": "renamed-store" }))
        .await;
    response.assert_status_ok();
    let updated: Value = response.json();
    assert_eq!(updated["name"], "renamed-store");

    let response = tenant_header(server.delete(&format!("/v1/stores/{store_id}")), tenant).await;
    response.assert_status(StatusCode::NO_CONTENT);

    let response = tenant_header(server.get(&format!("/v1/stores/{store_id}")), tenant).await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_store_not_found_returns_404() {
    let server = test_app().await;
    let response = tenant_header(
        server.get("/v1/stores/00000000-0000-0000-0000-000000000000"),
        "t1",
    )
    .await;

    response.assert_status(StatusCode::NOT_FOUND);
    let body: Value = response.json();
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_tenant_isolation_for_stores() {
    let server = test_app().await;

    let response = tenant_header(server.post("/v1/stores"), "tenant-a")
        .json(&json!({ "name": "private-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(server.get(&format!("/v1/stores/{store_id}")), "tenant-b").await;
    response.assert_status(StatusCode::NOT_FOUND);

    let response = tenant_header(server.get("/v1/stores"), "tenant-b").await;
    response.assert_status_ok();
    let stores: Vec<Value> = response.json();
    assert!(stores.is_empty());
}

#[tokio::test]
async fn test_memory_crud_and_listing() {
    let server = test_app().await;
    let tenant = "mem-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "mem-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "hello world" },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant },
        "tags": ["greeting"],
        "categories": []
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let memory: Value = response.json();
    let memory_id = memory["id"].as_str().unwrap();
    assert_eq!(memory["layer"], "semantic");

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .await;
    response.assert_status_ok();
    let fetched: Value = response.json();
    assert_eq!(fetched["content"]["text"], "hello world");

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .await;
    response.assert_status_ok();
    let page: Value = response.json();
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);

    let response = tenant_header(
        server.patch(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .json(&json!({
        "content": { "text": "updated" },
        "tags": ["greeting", "updated"]
    }))
    .await;
    response.assert_status_ok();
    let updated: Value = response.json();
    assert_eq!(updated["content"]["text"], "updated");
    assert_eq!(updated["version"], 2);

    let response = tenant_header(
        server.delete(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .await;
    response.assert_status(StatusCode::NO_CONTENT);

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_keyword_search() {
    let server = test_app().await;
    let tenant = "search-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "search-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "Rust systems programming language" },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant }
    }))
    .await;
    response.assert_status(StatusCode::CREATED);

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/search")),
        tenant,
    )
    .json(&json!({
        "query": "Rust programming",
        "keyword": true
    }))
    .await;
    response.assert_status_ok();
    let results: Vec<Value> = response.json();
    assert!(!results.is_empty());
    assert_eq!(
        results[0]["entry"]["content"]["text"],
        "Rust systems programming language"
    );
}

#[tokio::test]
async fn test_graph_endpoints() {
    let server = test_app().await;
    let tenant = "graph-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "graph-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let r1 = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "node A" },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant }
    }))
    .await;
    r1.assert_status(StatusCode::CREATED);
    let m1: Value = r1.json();

    let r2 = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "node B" },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant }
    }))
    .await;
    r2.assert_status(StatusCode::CREATED);
    let m2: Value = r2.json();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/graph/edges")),
        tenant,
    )
    .json(&json!({
        "source_memory_id": m1["id"],
        "target_memory_id": m2["id"],
        "relation_type": "related_to"
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let edge: Value = response.json();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/graph/traverse")),
        tenant,
    )
    .json(&json!({
        "start_memory_id": m1["id"],
        "depth": 1
    }))
    .await;
    response.assert_status_ok();
    let traversal: Value = response.json();
    assert_eq!(traversal["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(traversal["edges"].as_array().unwrap().len(), 1);

    let edge_id = edge["id"].as_str().unwrap();
    let response = tenant_header(
        server.delete(&format!("/v1/stores/{store_id}/graph/edges/{edge_id}")),
        tenant,
    )
    .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_gdpr_purge_requires_scope() {
    let server = test_app().await;
    let tenant = "gdpr-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "gdpr-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/gdpr/purge")),
        tenant,
    )
    .json(&json!({}))
    .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = response.json();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("requires at least one scope field"));
}

#[tokio::test]
async fn test_capabilities_endpoint() {
    let server = test_app().await;
    let response = server.get("/capabilities").await;

    response.assert_status_ok();
    let caps: Value = response.json();
    assert_eq!(caps["vector_search"], true);
    assert_eq!(caps["graph_support"], true);
    assert_eq!(caps["temporal_queries"], true);
    assert_eq!(caps["keyword_search"], true);
    assert_eq!(caps["batch_operations"], true);
    assert_eq!(caps["audit_log"], true);
    assert!(caps["max_batch_size"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_body_limit_rejects_oversized_payload() {
    let server = test_app().await;
    let tenant = "limit-tenant";

    // Create a store first
    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "limit-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    // Build a payload exceeding 10 MB
    let big_text = "x".repeat(11 * 1024 * 1024);
    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": big_text },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant }
    }))
    .await;

    response.assert_status(StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn test_tenant_isolation_for_memories() {
    let server = test_app().await;

    let response = tenant_header(server.post("/v1/stores"), "tenant-a")
        .json(&json!({ "name": "memory-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        "tenant-a",
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "tenant-a memory" },
        "owner_agent_id": "agent-a",
        "scope": { "tenant_id": "tenant-a" }
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let memory: Value = response.json();
    let memory_id = memory["id"].as_str().unwrap();

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        "tenant-b",
    )
    .await;
    response.assert_status(StatusCode::NOT_FOUND);

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories")),
        "tenant-b",
    )
    .await;
    response.assert_status_ok();
    let memories: Value = response.json();
    assert_eq!(memories["total"], 0);
    assert!(memories["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_batch_create_memories() {
    let server = test_app().await;
    let tenant = "batch-create-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "batch-create-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories/batch")),
        tenant,
    )
    .json(&json!({
        "entries": [
            {
                "layer": "semantic",
                "content": { "text": "memory-1" },
                "owner_agent_id": "agent-1",
                "scope": { "tenant_id": tenant }
            },
            {
                "layer": "episodic",
                "content": { "text": "memory-2" },
                "owner_agent_id": "agent-2",
                "scope": { "tenant_id": tenant }
            },
            {
                "layer": "working",
                "content": { "text": "memory-3" },
                "owner_agent_id": "agent-3",
                "scope": { "tenant_id": tenant }
            }
        ]
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    assert_eq!(body["created"].as_array().unwrap().len(), 3);
    assert!(body["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_batch_delete_memories() {
    let server = test_app().await;
    let tenant = "batch-delete-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "batch-delete-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "memory-1" },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant }
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let memory_1: Value = response.json();
    let memory_id_1 = memory_1["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "memory-2" },
        "owner_agent_id": "agent-2",
        "scope": { "tenant_id": tenant }
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let memory_2: Value = response.json();
    let memory_id_2 = memory_2["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories/batch/delete")),
        tenant,
    )
    .json(&json!({
        "memory_ids": [memory_id_1, memory_id_2]
    }))
    .await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["deleted"], 2);
    assert!(body["errors"].as_array().unwrap().is_empty());

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id_1}")),
        tenant,
    )
    .await;
    response.assert_status(StatusCode::NOT_FOUND);

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id_2}")),
        tenant,
    )
    .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_audit_log() {
    let server = test_app().await;
    let tenant = "audit-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "audit-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "audit me" },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant }
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let memory: Value = response.json();
    let memory_id = memory["id"].as_str().unwrap();

    let response = tenant_header(
        server.patch(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .json(&json!({
        "content": { "text": "updated audit me" }
    }))
    .await;
    response.assert_status_ok();

    let response = tenant_header(server.get(&format!("/v1/stores/{store_id}/audit")), tenant).await;
    response.assert_status_ok();
    let audit: Value = response.json();
    let items = audit["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(items.iter().any(|item| item["action"] == "create"));
    assert!(items.iter().any(|item| item["action"] == "update"));
}

#[tokio::test]
async fn test_memory_audit_log() {
    let server = test_app().await;
    let tenant = "memory-audit-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "memory-audit-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "audit this memory" },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant }
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let memory: Value = response.json();
    let memory_id = memory["id"].as_str().unwrap();

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id}/audit")),
        tenant,
    )
    .await;
    response.assert_status_ok();
    let audit: Value = response.json();
    let items = audit["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(items.iter().any(|item| item["action"] == "create"));
}

#[tokio::test]
async fn test_purge_expired() {
    let server = test_app().await;
    let tenant = "purge-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "purge-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "expired memory" },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant },
        "expires_at": "2020-01-01T00:00:00Z"
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let memory: Value = response.json();
    let memory_id = memory["id"].as_str().unwrap();

    let response = tenant_header(
        server.delete(&format!("/v1/stores/{store_id}/expired")),
        tenant,
    )
    .await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["deleted"], 1);

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_invalid_uuid_returns_400() {
    let server = test_app().await;
    let response = tenant_header(server.get("/v1/stores/not-a-uuid"), "uuid-tenant").await;

    response.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = response.json();
    assert!(body["error"].is_string(), "expected JSON error response");
}

#[tokio::test]
async fn test_gdpr_purge_deletes_and_anonymizes_audit() {
    let server = test_app().await;
    let tenant = "gdpr-anon-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "gdpr-anon-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "pii memory" },
        "owner_agent_id": "agent-1",
        "scope": {
            "tenant_id": tenant,
            "user_id": "user123"
        }
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let memory: Value = response.json();
    let memory_id = memory["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/gdpr/purge")),
        tenant,
    )
    .json(&json!({ "user_id": "user123" }))
    .await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["deleted"], 1);

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .await;
    response.assert_status(StatusCode::NOT_FOUND);

    let response = tenant_header(server.get(&format!("/v1/stores/{store_id}/audit")), tenant).await;
    response.assert_status_ok();
    let audit: Value = response.json();
    let items: Vec<&Value> = audit["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["memory_id"] == memory_id)
        .collect();
    assert!(!items.is_empty());
    for item in items {
        assert!(item["agent_id"].is_null());
        assert!(item["details"].is_null());
    }
}

#[tokio::test]
async fn test_update_memory_clears_expires_at() {
    let server = test_app().await;
    let tenant = "clear-expiry-tenant";

    let response = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "clear-expiry-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "layer": "semantic",
        "content": { "text": "expiring memory" },
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant },
        "expires_at": "2030-01-01T00:00:00Z"
    }))
    .await;
    response.assert_status(StatusCode::CREATED);
    let memory: Value = response.json();
    let memory_id = memory["id"].as_str().unwrap();
    assert_eq!(memory["expires_at"], "2030-01-01T00:00:00Z");

    let response = tenant_header(
        server.patch(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .json(&json!({
        "expires_at": null
    }))
    .await;
    response.assert_status_ok();
    let updated: Value = response.json();
    assert!(updated["expires_at"].is_null());

    let response = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .await;
    response.assert_status_ok();
    let fetched: Value = response.json();
    assert!(fetched["expires_at"].is_null());
}

#[tokio::test]
async fn test_malformed_json_body_returns_json_error() {
    let server = test_app().await;
    let response = tenant_header(server.post("/v1/stores"), "json-tenant")
        .add_header(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        )
        .bytes(axum::body::Bytes::from_static(b"{invalid json"))
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = response.json();
    assert!(body["error"].is_string(), "expected JSON error response");
}

// --- Tests for OMS spec 4.1.1, 4.3.2, 4.4.1 ---

#[tokio::test]
async fn test_default_tenant_creates_store_without_header() {
    let server = test_app().await;

    let response = server
        .post("/v1/stores")
        .json(&json!({ "name": "auto-store" }))
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    assert_eq!(body["name"], "auto-store");
    assert_eq!(body["tenant_id"], "_default");
}

#[tokio::test]
async fn test_default_store_alias_auto_creates_store() {
    let server = test_app().await;

    let response = server
        .post("/v1/stores/_default/memories")
        .json(&json!({
            "content": "test memory via default store",
            "owner_agent_id": "agent-1",
            "scope": { "tenant_id": "_default" }
        }))
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    assert_eq!(body["content"], "test memory via default store");
    assert_eq!(body["owner_agent_id"], "agent-1");
}

#[tokio::test]
async fn test_default_store_reuses_existing_default() {
    let server = test_app().await;

    // First write auto-creates the default store
    let first = server
        .post("/v1/stores/_default/memories")
        .json(&json!({
            "content": "first",
            "owner_agent_id": "agent-1",
            "scope": { "tenant_id": "_default" }
        }))
        .await;
    first.assert_status(StatusCode::CREATED);
    let first_store_id = first.json::<Value>()["store_id"].clone();

    // Second write reuses the same default store
    let second = server
        .post("/v1/stores/_default/memories")
        .json(&json!({
            "content": "second",
            "owner_agent_id": "agent-1",
            "scope": { "tenant_id": "_default" }
        }))
        .await;
    second.assert_status(StatusCode::CREATED);
    let second_store_id = second.json::<Value>()["store_id"].clone();

    assert_eq!(first_store_id, second_store_id);
}

#[tokio::test]
async fn test_default_store_disabled_returns_error() {
    let server = test_app_strict().await;
    let tenant = "test-tenant";

    let response = tenant_header(server.post("/v1/stores/_default/memories"), tenant)
        .json(&json!({
            "content": "should fail",
            "owner_agent_id": "agent-1",
            "scope": { "tenant_id": tenant }
        }))
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = response.json();
    assert!(body["error"].as_str().unwrap().contains("_default"));
}

#[tokio::test]
async fn test_upsert_creates_new_entry() {
    let server = test_app().await;
    let tenant = "test-tenant";

    // Create a store
    let store_resp = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "upsert-store" }))
        .await;
    let store_id = store_resp.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create memory with upsert_key
    let response = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "content": "dark mode",
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant },
        "upsert_key": "preference:theme"
    }))
    .await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    assert_eq!(body["content"], "dark mode");
    assert_eq!(body["version"], 1);
    assert_eq!(body["upsert_key"], "preference:theme");
}

#[tokio::test]
async fn test_upsert_replaces_existing_entry() {
    let server = test_app().await;
    let tenant = "test-tenant";

    let store_resp = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "upsert-store" }))
        .await;
    let store_id = store_resp.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // First upsert
    let first = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "content": "dark mode",
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant },
        "upsert_key": "preference:theme"
    }))
    .await;
    first.assert_status(StatusCode::CREATED);
    let first_id = first.json::<Value>()["id"].as_str().unwrap().to_string();

    // Second upsert with same key -- should replace
    let second = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "content": "light mode",
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant },
        "upsert_key": "preference:theme"
    }))
    .await;
    second.assert_status(StatusCode::CREATED);
    let second_body: Value = second.json();

    // Same ID, incremented version, new content
    assert_eq!(second_body["id"], first_id);
    assert_eq!(second_body["content"], "light mode");
    assert_eq!(second_body["version"], 2);
}

#[tokio::test]
async fn test_upsert_different_keys_create_separate_entries() {
    let server = test_app().await;
    let tenant = "test-tenant";

    let store_resp = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "upsert-store" }))
        .await;
    let store_id = store_resp.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let first = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "content": "dark mode",
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant },
        "upsert_key": "preference:theme"
    }))
    .await;

    let second = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "content": "english",
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant },
        "upsert_key": "preference:language"
    }))
    .await;

    let first_id = first.json::<Value>()["id"].as_str().unwrap().to_string();
    let second_id = second.json::<Value>()["id"].as_str().unwrap().to_string();

    assert_ne!(first_id, second_id);
}

#[tokio::test]
async fn test_plain_string_content_round_trips() {
    let server = test_app().await;
    let tenant = "test-tenant";

    let store_resp = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "content-test" }))
        .await;
    let store_id = store_resp.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create with plain string content
    let created = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "content": "just a plain string",
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant }
    }))
    .await;
    created.assert_status(StatusCode::CREATED);
    let memory_id = created.json::<Value>()["id"].as_str().unwrap().to_string();

    // Read it back
    let fetched = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .await;
    let body: Value = fetched.json();

    // Content should be a plain string, not wrapped in {"text": "..."}
    assert_eq!(body["content"], "just a plain string");
}

#[tokio::test]
async fn test_structured_content_round_trips() {
    let server = test_app().await;
    let tenant = "test-tenant";

    let store_resp = tenant_header(server.post("/v1/stores"), tenant)
        .json(&json!({ "name": "content-test" }))
        .await;
    let store_id = store_resp.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let structured = json!({"key": "value", "nested": {"a": 1}});
    let created = tenant_header(
        server.post(&format!("/v1/stores/{store_id}/memories")),
        tenant,
    )
    .json(&json!({
        "content": structured,
        "owner_agent_id": "agent-1",
        "scope": { "tenant_id": tenant }
    }))
    .await;
    created.assert_status(StatusCode::CREATED);
    let memory_id = created.json::<Value>()["id"].as_str().unwrap().to_string();

    let fetched = tenant_header(
        server.get(&format!("/v1/stores/{store_id}/memories/{memory_id}")),
        tenant,
    )
    .await;
    let body: Value = fetched.json();
    assert_eq!(body["content"], structured);
}

#[tokio::test]
async fn test_zero_setup_write_with_defaults() {
    let server = test_app().await;

    // No tenant header, no store creation -- just write a memory
    let response = server
        .post("/v1/stores/_default/memories")
        .json(&json!({
            "content": "zero setup memory",
            "owner_agent_id": "agent-1",
            "scope": {}
        }))
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    assert_eq!(body["content"], "zero setup memory");
    // Scope should have been normalized to the default tenant
    assert_eq!(body["scope"]["tenant_id"], "_default");
}

// ── Embedding integration tests ─────────────────────────────────────────────

/// Deterministic fake embedder for tests (avoids slow model download).
struct FakeEmbedder;

#[async_trait::async_trait]
impl kd6_core::EmbeddingProvider for FakeEmbedder {
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

/// Test app with deterministic fake embedder for embedding tests.
async fn test_app_with_embedder() -> TestServer {
    let provider = SqliteProvider::new("sqlite::memory:").await.unwrap();
    let state = AppState {
        provider: Arc::new(provider),
        embedder: Arc::new(FakeEmbedder),
        config: ServerConfig::default(),
    };

    let app = kd6_server::build_router(state);
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn test_auto_embed_on_write() {
    let server = test_app_with_embedder().await;

    // Create a store
    let response = server
        .post("/v1/stores")
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-test"),
        )
        .json(&json!({
            "name": "embed-store",
            "region": "local"
        }))
        .await;
    let store_id = response.json::<Value>()["id"].as_str().unwrap().to_string();

    // Create memory WITHOUT providing embedding
    let response = server
        .post(&format!("/v1/stores/{store_id}/memories"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-test"),
        )
        .json(&json!({
            "content": "The user prefers dark mode in all applications",
            "owner_agent_id": "test-agent",
            "scope": {}
        }))
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();

    // Embedding should have been auto-computed
    let embedding = body["embedding"]
        .as_array()
        .expect("embedding should be present");
    assert!(!embedding.is_empty(), "embedding should not be empty");
    assert_eq!(embedding.len(), 3, "FakeEmbedder produces 3-dim vectors");
}

#[tokio::test]
async fn test_auto_embed_on_search() {
    let server = test_app_with_embedder().await;

    // Create store + memory (embedding auto-computed on write)
    let response = server
        .post("/v1/stores")
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-search"),
        )
        .json(&json!({
            "name": "search-store",
            "region": "local"
        }))
        .await;
    let store_id = response.json::<Value>()["id"].as_str().unwrap().to_string();

    // Create a few memories
    for content in [
        "The team decided to use PostgreSQL for the database",
        "User interface should follow Material Design guidelines",
        "Authentication will use OAuth 2.0 with JWT tokens",
    ] {
        server
            .post(&format!("/v1/stores/{store_id}/memories"))
            .add_header(
                HeaderName::from_static("x-tenant-id"),
                HeaderValue::from_static("embed-search"),
            )
            .json(&json!({
                "content": content,
                "owner_agent_id": "test-agent",
                "scope": {}
            }))
            .await;
    }

    // Search WITHOUT providing embedding — should auto-embed the query
    let response = server
        .post(&format!("/v1/stores/{store_id}/search"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-search"),
        )
        .json(&json!({
            "query": "database choice",
            "top_k": 3,
            "threshold": 0.0
        }))
        .await;

    response.assert_status_ok();
    let results: Vec<Value> = response.json();

    // Should find results via vector similarity
    assert!(!results.is_empty(), "vector search should return results");
}

#[tokio::test]
async fn test_auto_embed_preserves_caller_embedding() {
    let server = test_app_with_embedder().await;

    let response = server
        .post("/v1/stores")
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-preserve"),
        )
        .json(&json!({
            "name": "preserve-store",
            "region": "local"
        }))
        .await;
    let store_id = response.json::<Value>()["id"].as_str().unwrap().to_string();

    // Provide a custom 3-dim embedding (matches FakeEmbedder dimensions)
    let custom_embedding: Vec<f32> = vec![42.0, 21.0, 1.0];

    let response = server
        .post(&format!("/v1/stores/{store_id}/memories"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-preserve"),
        )
        .json(&json!({
            "content": "test content",
            "owner_agent_id": "test-agent",
            "scope": {},
            "embedding": custom_embedding
        }))
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();

    // Should use the caller-provided embedding, not auto-compute
    let stored: Vec<f64> = body["embedding"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert_eq!(stored.len(), 3);
    // Check first value matches what we sent
    assert!((stored[0] - 42.0).abs() < 0.001);
    assert!((stored[1] - 21.0).abs() < 0.001);
}

#[tokio::test]
async fn test_auto_embed_rejects_wrong_dimensions() {
    let server = test_app_with_embedder().await;

    let response = server
        .post("/v1/stores")
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-dim"),
        )
        .json(&json!({
            "name": "dim-store",
            "region": "local"
        }))
        .await;
    let store_id = response.json::<Value>()["id"].as_str().unwrap().to_string();

    // Send a 100-dim embedding when model expects 3
    let wrong_embedding: Vec<f32> = vec![0.1; 100];

    let response = server
        .post(&format!("/v1/stores/{store_id}/memories"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-dim"),
        )
        .json(&json!({
            "content": "test content",
            "owner_agent_id": "test-agent",
            "scope": {},
            "embedding": wrong_embedding
        }))
        .await;

    // Should be rejected due to dimensionality mismatch
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_auto_embed_on_update() {
    let server = test_app_with_embedder().await;

    let response = server
        .post("/v1/stores")
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-update"),
        )
        .json(&json!({
            "name": "update-store",
            "region": "local"
        }))
        .await;
    let store_id = response.json::<Value>()["id"].as_str().unwrap().to_string();

    // Create memory
    let response = server
        .post(&format!("/v1/stores/{store_id}/memories"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-update"),
        )
        .json(&json!({
            "content": "cats are great pets",
            "owner_agent_id": "test-agent",
            "scope": {}
        }))
        .await;
    let memory_id = response.json::<Value>()["id"].as_str().unwrap().to_string();
    let original_embedding: Vec<f64> = response.json::<Value>()["embedding"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();

    // Update content — embedding should be recomputed
    let response = server
        .patch(&format!("/v1/stores/{store_id}/memories/{memory_id}"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("embed-update"),
        )
        .json(&json!({
            "content": "quantum computing breakthroughs in 2026"
        }))
        .await;

    response.assert_status_ok();
    let updated_embedding: Vec<f64> = response.json::<Value>()["embedding"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();

    assert_eq!(updated_embedding.len(), 3);
    // Embedding should be different because content changed
    assert_ne!(
        original_embedding, updated_embedding,
        "embedding should change when content changes"
    );
}

#[tokio::test]
async fn test_noop_embedder_passthrough() {
    // Using test_app() which has NoopEmbedder
    let server = test_app().await;

    let response = server
        .post("/v1/stores/_default/memories")
        .json(&json!({
            "content": "no embedding provider configured",
            "owner_agent_id": "test-agent",
            "scope": {}
        }))
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    // With NoopEmbedder, no embedding should be computed
    assert!(
        body["embedding"].is_null(),
        "noop embedder should not produce embeddings"
    );
}

// ---------------------------------------------------------------------------
// Vector search via HTTP
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_vector_search_returns_ranked_results() {
    let server = test_app_with_embedder().await;

    let response = server
        .post("/v1/stores")
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("vsearch-test"),
        )
        .json(&json!({"name": "vsearch-store", "region": "local"}))
        .await;
    let store_id = response.json::<Value>()["id"].as_str().unwrap().to_string();

    // Add semantically diverse documents
    for text in [
        "The Eiffel Tower is a landmark in Paris, France",
        "Machine learning models require training data",
        "The Louvre Museum contains the Mona Lisa",
        "Rust is a systems programming language",
    ] {
        server
            .post(&format!("/v1/stores/{store_id}/memories"))
            .add_header(
                HeaderName::from_static("x-tenant-id"),
                HeaderValue::from_static("vsearch-test"),
            )
            .json(&json!({
                "content": text,
                "owner_agent_id": "test-agent",
                "scope": {}
            }))
            .await;
    }

    // Vector search (no keyword=true, so pure vector similarity)
    let response = server
        .post(&format!("/v1/stores/{store_id}/search"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("vsearch-test"),
        )
        .json(&json!({"query": "Paris landmarks and attractions", "top_k": 4}))
        .await;
    response.assert_status_ok();
    let results: Vec<Value> = response.json();

    assert!(!results.is_empty(), "vector search should return results");
    // All results should have scores
    for r in &results {
        assert!(r["score"].as_f64().is_some(), "result should have a score");
    }
}

#[tokio::test]
async fn test_structured_content_with_arrays_embeds_correctly() {
    let server = test_app_with_embedder().await;

    let response = server
        .post("/v1/stores")
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("array-test"),
        )
        .json(&json!({"name": "array-store", "region": "local"}))
        .await;
    let store_id = response.json::<Value>()["id"].as_str().unwrap().to_string();

    // Create memory with array content containing string values
    let response = server
        .post(&format!("/v1/stores/{store_id}/memories"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("array-test"),
        )
        .json(&json!({
            "content": {
                "title": "Array Test",
                "items": ["artificial intelligence", "machine learning", "deep learning"],
                "nested": {"tags": ["neural networks"]}
            },
            "owner_agent_id": "test-agent",
            "scope": {}
        }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    // Should have an embedding (arrays were traversed for text)
    assert!(
        body["embedding"].is_array(),
        "structured content with arrays should produce embedding"
    );
}

#[tokio::test]
async fn test_update_memory_explicit_clear_embedding() {
    let server = test_app_with_embedder().await;

    let response = server
        .post("/v1/stores")
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("clear-test"),
        )
        .json(&json!({"name": "clear-store", "region": "local"}))
        .await;
    let store_id = response.json::<Value>()["id"].as_str().unwrap().to_string();

    // Create memory (auto-embedded)
    let response = server
        .post(&format!("/v1/stores/{store_id}/memories"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("clear-test"),
        )
        .json(&json!({
            "content": "this will be embedded",
            "owner_agent_id": "test-agent",
            "scope": {}
        }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: Value = response.json();
    let memory_id = created["id"].as_str().unwrap();
    assert!(created["embedding"].is_array(), "should have auto-embedded");

    // Update with content change but no explicit embedding — should auto-recompute
    let response = server
        .patch(&format!("/v1/stores/{store_id}/memories/{memory_id}"))
        .add_header(
            HeaderName::from_static("x-tenant-id"),
            HeaderValue::from_static("clear-test"),
        )
        .json(&json!({
            "content": "updated content for re-embedding",
            "version": 1
        }))
        .await;
    response.assert_status_ok();
    let updated: Value = response.json();
    assert!(
        updated["embedding"].is_array(),
        "should have re-computed embedding on content change"
    );
}

#[tokio::test]
async fn test_inheritance_create_and_delete() {
    let server = test_app().await;

    let response = server
        .post("/v1/stores")
        .json(&json!({ "name": "inheritance-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = server
        .post(&format!("/v1/stores/{store_id}/inherit"))
        .json(&json!({
            "parent_agent_id": "parent-agent",
            "child_agent_id": "child-agent",
            "inherit_layers": ["working", "episodic"],
            "filter": {},
            "bubble_up": {"enabled": true, "layers": ["working"]},
            "access": "read_only"
        }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let inheritance: Value = response.json();
    let inheritance_id = inheritance["id"].as_str().unwrap();

    assert_eq!(inheritance["store_id"], store_id);
    assert_eq!(inheritance["tenant_id"], "_default");
    assert_eq!(inheritance["parent_agent_id"], "parent-agent");
    assert_eq!(inheritance["child_agent_id"], "child-agent");
    assert_eq!(
        inheritance["inherit_layers"],
        json!(["working", "episodic"])
    );
    assert_eq!(inheritance["bubble_up"]["enabled"], true);
    assert_eq!(inheritance["bubble_up"]["layers"], json!(["working"]));
    assert_eq!(inheritance["access"], "read_only");

    let response = server
        .delete(&format!("/v1/stores/{store_id}/inherit/{inheritance_id}"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_bubble_up_creates_parent_memories() {
    let server = test_app().await;

    let response = server
        .post("/v1/stores")
        .json(&json!({ "name": "bubble-up-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = server
        .post(&format!("/v1/stores/{store_id}/inherit"))
        .json(&json!({
            "parent_agent_id": "parent-agent",
            "child_agent_id": "child-agent",
            "inherit_layers": ["working", "episodic"],
            "filter": {},
            "bubble_up": {"enabled": true, "layers": ["working"]},
            "access": "read_only"
        }))
        .await;
    response.assert_status(StatusCode::CREATED);

    for content in ["child memory one", "child memory two"] {
        let response = server
            .post(&format!("/v1/stores/{store_id}/memories"))
            .json(&json!({
                "layer": "working",
                "content": { "text": content },
                "owner_agent_id": "child-agent",
                "scope": { "agent_id": "child-agent" }
            }))
            .await;
        response.assert_status(StatusCode::CREATED);
    }

    let response = server
        .post(&format!("/v1/stores/{store_id}/bubble-up"))
        .json(&json!({
            "parent_agent_id": "parent-agent",
            "child_agent_id": "child-agent",
            "layers": ["working"]
        }))
        .await;
    response.assert_status_ok();
    let bubbled: Value = response.json();
    let items = bubbled.as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items
        .iter()
        .all(|item| item["owner_agent_id"] == "parent-agent"));
    assert!(items
        .iter()
        .all(|item| item["scope"]["agent_id"] == "parent-agent"));
    assert!(items.iter().all(|item| item["source"]["uri"]
        .as_str()
        .unwrap()
        .starts_with("bubble_up:")));

    let response = server
        .get(&format!(
            "/v1/stores/{store_id}/memories?owner_agent_id=parent-agent"
        ))
        .await;
    response.assert_status_ok();
    let memories: Value = response.json();
    let parent_items = memories["items"].as_array().unwrap();
    assert_eq!(memories["total"], 2);
    assert_eq!(parent_items.len(), 2);
    assert!(parent_items
        .iter()
        .all(|item| item["content"]["text"] == "child memory one"
            || item["content"]["text"] == "child memory two"));
}

#[tokio::test]
async fn test_shared_space_lifecycle() {
    let server = test_app().await;

    let response = server
        .post("/v1/stores")
        .json(&json!({ "name": "shared-space-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = server
        .post(&format!("/v1/stores/{store_id}/shared-spaces"))
        .json(&json!({
            "name": "test-space",
            "description": "A test shared space",
            "allowed_layers": ["working", "semantic"],
            "creator_agent_id": "agent-1",
            "scope": {},
            "layer": "working"
        }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let space: Value = response.json();
    let space_id = space["id"].as_str().unwrap();

    assert_eq!(space["name"], "test-space");
    assert_eq!(space["tenant_id"], "_default");
    assert_eq!(space["layer"], "working");
    assert_eq!(space["scope"]["tenant_id"], "_default");
    assert!(space["participants"].as_array().unwrap().is_empty());

    let response = server
        .get(&format!("/v1/stores/{store_id}/shared-spaces"))
        .await;
    response.assert_status_ok();
    let spaces: Value = response.json();
    let listed_spaces = spaces.as_array().unwrap();
    assert_eq!(listed_spaces.len(), 1);
    assert_eq!(listed_spaces[0]["id"], space_id);

    let response = server
        .get(&format!("/v1/stores/{store_id}/shared-spaces/{space_id}"))
        .await;
    response.assert_status_ok();
    let fetched: Value = response.json();
    assert_eq!(fetched["id"], space_id);
    assert_eq!(fetched["name"], "test-space");
    assert_eq!(fetched["layer"], "working");

    let response = server
        .post(&format!(
            "/v1/stores/{store_id}/shared-spaces/{space_id}/join"
        ))
        .json(&json!({
            "agent_id": "agent-2",
            "access": "read_write"
        }))
        .await;
    response.assert_status_ok();
    let joined: Value = response.json();
    let participants = joined["participants"].as_array().unwrap();
    assert_eq!(participants.len(), 1);
    assert_eq!(participants[0]["agent_id"], "agent-2");
    assert_eq!(participants[0]["access"], "read_write");

    let response = server
        .post(&format!(
            "/v1/stores/{store_id}/shared-spaces/{space_id}/leave"
        ))
        .json(&json!({
            "agent_id": "agent-2"
        }))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    let response = server
        .delete(&format!("/v1/stores/{store_id}/shared-spaces/{space_id}"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    let response = server
        .get(&format!("/v1/stores/{store_id}/shared-spaces"))
        .await;
    response.assert_status_ok();
    let spaces: Value = response.json();
    assert!(spaces.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_inheritance_not_found_returns_error() {
    let server = test_app().await;

    let response = server
        .post("/v1/stores")
        .json(&json!({ "name": "missing-inheritance-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = server
        .delete(&format!(
            "/v1/stores/{store_id}/inherit/00000000-0000-0000-0000-000000000001"
        ))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = response.json();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("inheritance not found"));
}

#[tokio::test]
async fn test_shared_space_not_found_returns_error() {
    let server = test_app().await;

    let response = server
        .post("/v1/stores")
        .json(&json!({ "name": "missing-space-store" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let store: Value = response.json();
    let store_id = store["id"].as_str().unwrap();

    let response = server
        .get(&format!(
            "/v1/stores/{store_id}/shared-spaces/00000000-0000-0000-0000-000000000001"
        ))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = response.json();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("shared space not found"));
}

use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum_test::TestServer;
use serde_json::{json, Value};

use kd6_server::state::AppState;
use kd6_sqlite::SqliteProvider;

async fn test_app() -> TestServer {
    let provider = SqliteProvider::new("sqlite::memory:").await.unwrap();
    let state = AppState {
        provider: Arc::new(provider),
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
    let server = test_app().await;
    let response = server.get("/v1/stores").await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    let body: Value = response.json();
    assert!(body["error"].as_str().unwrap().contains("X-Tenant-ID"));
}

#[tokio::test]
async fn test_empty_tenant_header_returns_401() {
    let server = test_app().await;
    let response = tenant_header(server.get("/v1/stores"), "").await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    let body: Value = response.json();
    assert!(body["error"].as_str().unwrap().contains("X-Tenant-ID"));
}

#[tokio::test]
async fn test_whitespace_tenant_header_returns_401() {
    let server = test_app().await;
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

use async_trait::async_trait;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use uuid::Uuid;

use kd6_core::error::OmsError;
use kd6_core::models::*;
use kd6_core::OmsProvider;

#[cfg(test)]
pub(crate) use crate::helpers::{bytes_to_embedding, cosine_similarity, embedding_to_bytes};
#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use sqlx::Row;

/// SQLite-backed OMS provider for local development, testing, and embedded use.
pub struct SqliteProvider {
    pub(crate) pool: SqlitePool,
}

impl SqliteProvider {
    pub async fn new(database_url: &str) -> Result<Self, OmsError> {
        use sqlx::sqlite::SqliteConnectOptions;
        use std::str::FromStr;

        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(|e| OmsError::Internal(format!("invalid database URL: {e}")))?
            .pragma("journal_mode", "WAL")
            .pragma("foreign_keys", "ON")
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to connect to SQLite: {e}")))?;

        let provider = Self { pool };
        provider.run_migrations().await?;
        Ok(provider)
    }

    async fn run_migrations(&self) -> Result<(), OmsError> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("migration failed: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl OmsProvider for SqliteProvider {
    async fn create_store(
        &self,
        tenant_id: &str,
        request: CreateStoreRequest,
    ) -> Result<MemoryStore, OmsError> {
        crate::stores::create_store(&self.pool, tenant_id, request).await
    }

    async fn get_store(&self, tenant_id: &str, store_id: Uuid) -> Result<MemoryStore, OmsError> {
        crate::stores::get_store(&self.pool, tenant_id, store_id).await
    }

    async fn list_stores(&self, tenant_id: &str) -> Result<Vec<MemoryStore>, OmsError> {
        crate::stores::list_stores(&self.pool, tenant_id).await
    }

    async fn get_store_by_name(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> Result<MemoryStore, OmsError> {
        crate::stores::get_store_by_name(&self.pool, tenant_id, name).await
    }

    async fn get_or_create_store(
        &self,
        tenant_id: &str,
        name: &str,
        request: CreateStoreRequest,
    ) -> Result<MemoryStore, OmsError> {
        crate::stores::get_or_create_store(&self.pool, tenant_id, name, request).await
    }

    async fn update_store(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: UpdateStoreRequest,
    ) -> Result<MemoryStore, OmsError> {
        crate::stores::update_store(&self.pool, tenant_id, store_id, request).await
    }

    async fn delete_store(&self, tenant_id: &str, store_id: Uuid) -> Result<(), OmsError> {
        crate::stores::delete_store(&self.pool, tenant_id, store_id).await
    }

    async fn create_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateMemoryRequest,
    ) -> Result<MemoryEntry, OmsError> {
        crate::memories::create_memory(&self.pool, tenant_id, store_id, request).await
    }

    async fn get_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Uuid,
    ) -> Result<MemoryEntry, OmsError> {
        crate::memories::get_memory(&self.pool, tenant_id, store_id, memory_id).await
    }

    async fn list_memories(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        filter: ListMemoriesFilter,
    ) -> Result<Page<MemoryEntry>, OmsError> {
        crate::memories::list_memories(&self.pool, tenant_id, store_id, filter).await
    }

    async fn update_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Uuid,
        request: UpdateMemoryRequest,
    ) -> Result<MemoryEntry, OmsError> {
        crate::memories::update_memory(&self.pool, tenant_id, store_id, memory_id, request).await
    }

    async fn delete_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Uuid,
    ) -> Result<(), OmsError> {
        crate::memories::delete_memory(&self.pool, tenant_id, store_id, memory_id).await
    }

    async fn search(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        query: SearchQuery,
    ) -> Result<Vec<SearchResult>, OmsError> {
        crate::search::search(&self.pool, tenant_id, store_id, query).await
    }

    async fn audit_log(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        filter: AuditFilter,
    ) -> Result<Page<AuditEntry>, OmsError> {
        crate::audit::audit_log(&self.pool, tenant_id, store_id, filter).await
    }

    async fn purge_expired(&self, tenant_id: &str, store_id: Uuid) -> Result<u64, OmsError> {
        crate::lifecycle::purge_expired(&self.pool, tenant_id, store_id).await
    }

    async fn batch_create_memories(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: BatchCreateRequest,
    ) -> Result<BatchCreateResponse, OmsError> {
        crate::lifecycle::batch_create_memories(&self.pool, tenant_id, store_id, request).await
    }

    async fn batch_delete_memories(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: BatchDeleteRequest,
    ) -> Result<BatchDeleteResponse, OmsError> {
        crate::lifecycle::batch_delete_memories(&self.pool, tenant_id, store_id, request).await
    }

    async fn create_inheritance(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateInheritanceRequest,
    ) -> Result<InheritanceSpec, OmsError> {
        crate::inheritance::create_inheritance(&self.pool, tenant_id, store_id, request).await
    }

    async fn delete_inheritance(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        inheritance_id: Uuid,
    ) -> Result<(), OmsError> {
        crate::inheritance::delete_inheritance(&self.pool, tenant_id, store_id, inheritance_id)
            .await
    }

    async fn bubble_up(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: kd6_core::models::BubbleUpRequest,
    ) -> Result<Vec<MemoryEntry>, OmsError> {
        crate::inheritance::bubble_up(&self.pool, tenant_id, store_id, request).await
    }

    async fn create_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateSharedSpaceRequest,
    ) -> Result<SharedSpace, OmsError> {
        crate::shared_spaces::create_shared_space(&self.pool, tenant_id, store_id, request).await
    }

    async fn list_shared_spaces(
        &self,
        tenant_id: &str,
        store_id: Uuid,
    ) -> Result<Vec<SharedSpace>, OmsError> {
        crate::shared_spaces::list_shared_spaces(&self.pool, tenant_id, store_id).await
    }

    async fn get_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
    ) -> Result<SharedSpace, OmsError> {
        crate::shared_spaces::get_shared_space(&self.pool, tenant_id, store_id, space_id).await
    }

    async fn join_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
        request: JoinSpaceRequest,
    ) -> Result<SharedSpace, OmsError> {
        crate::shared_spaces::join_shared_space(&self.pool, tenant_id, store_id, space_id, request)
            .await
    }

    async fn leave_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
        request: LeaveSpaceRequest,
    ) -> Result<(), OmsError> {
        crate::shared_spaces::leave_shared_space(&self.pool, tenant_id, store_id, space_id, request)
            .await
    }

    async fn delete_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
    ) -> Result<(), OmsError> {
        crate::shared_spaces::delete_shared_space(&self.pool, tenant_id, store_id, space_id).await
    }

    async fn create_edge(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateEdgeRequest,
    ) -> Result<GraphEdge, OmsError> {
        crate::graph::create_edge(&self.pool, tenant_id, store_id, request).await
    }

    async fn delete_edge(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        edge_id: Uuid,
    ) -> Result<(), OmsError> {
        crate::graph::delete_edge(&self.pool, tenant_id, store_id, edge_id).await
    }

    async fn graph_traverse(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: GraphTraversalRequest,
    ) -> Result<GraphTraversalResult, OmsError> {
        crate::graph::graph_traverse(&self.pool, tenant_id, store_id, request).await
    }

    async fn gdpr_purge(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        scope: MemoryScope,
    ) -> Result<u64, OmsError> {
        crate::gdpr::gdpr_purge(&self.pool, tenant_id, store_id, scope).await
    }

    async fn stats(&self, tenant_id: &str, store_id: Uuid) -> Result<StoreStats, OmsError> {
        crate::lifecycle::stats(&self.pool, tenant_id, store_id).await
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supported_layers: vec![
                MemoryLayer::Working,
                MemoryLayer::Episodic,
                MemoryLayer::Semantic,
                MemoryLayer::Procedural,
                MemoryLayer::Archival,
            ],
            vector_search: true,
            graph_support: true,
            temporal_queries: true,
            keyword_search: true,
            max_embedding_dimensions: None,
            supported_distance_metrics: vec!["cosine".into()],
            compaction_support: false,
            archival_support: false,
            max_entry_size_bytes: None,
            batch_operations: true,
            max_batch_size: Some(1000),
            pub_sub_notifications: false,
            encryption_at_rest: false,
            audit_log: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use kd6_core::models::{CreateStoreRequest, StoreConfig, UpdateStoreRequest};

    async fn test_provider() -> SqliteProvider {
        SqliteProvider::new("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_and_get_store() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "my-store".into(),
                    region: Some("us-east".into()),
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        assert_eq!(store.name, "my-store");
        assert_eq!(store.tenant_id, "tenant-1");
        assert_eq!(store.region.as_deref(), Some("us-east"));

        let fetched = provider.get_store("tenant-1", store.id).await.unwrap();
        assert_eq!(fetched.id, store.id);
        assert_eq!(fetched.name, "my-store");
    }

    #[tokio::test]
    async fn get_store_wrong_tenant_returns_not_found() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "secret-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let result = provider.get_store("tenant-2", store.id).await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn list_stores_filters_by_tenant() {
        let provider = test_provider().await;
        for i in 0..3 {
            provider
                .create_store(
                    "tenant-a",
                    CreateStoreRequest {
                        name: format!("store-a-{i}"),
                        region: None,
                        config: StoreConfig::default(),
                        metadata: Default::default(),
                    },
                )
                .await
                .unwrap();
        }
        provider
            .create_store(
                "tenant-b",
                CreateStoreRequest {
                    name: "store-b".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let a_stores = provider.list_stores("tenant-a").await.unwrap();
        assert_eq!(a_stores.len(), 3);

        let b_stores = provider.list_stores("tenant-b").await.unwrap();
        assert_eq!(b_stores.len(), 1);

        let empty = provider.list_stores("tenant-c").await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn update_store_partial() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "original".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let updated = provider
            .update_store(
                "tenant-1",
                store.id,
                UpdateStoreRequest {
                    config: Some(StoreConfig {
                        default_ttl_seconds: Some(3600),
                        ..Default::default()
                    }),
                    metadata: None,
                },
            )
            .await
            .unwrap();

        // Name is immutable — should not change
        assert_eq!(updated.name, "original");
        assert_eq!(updated.config.default_ttl_seconds, Some(3600));
        assert!(updated.updated_at > store.updated_at);
    }

    #[tokio::test]
    async fn update_store_wrong_tenant() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "mine".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let result = provider
            .update_store(
                "tenant-2",
                store.id,
                UpdateStoreRequest {
                    config: None,
                    metadata: Some([("evil".into(), "true".into())].into()),
                },
            )
            .await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn delete_store_success() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "doomed".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        provider.delete_store("tenant-1", store.id).await.unwrap();

        let result = provider.get_store("tenant-1", store.id).await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn delete_store_wrong_tenant() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "protected".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let result = provider.delete_store("tenant-2", store.id).await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));

        // Verify it still exists for the correct tenant
        provider.get_store("tenant-1", store.id).await.unwrap();
    }

    #[tokio::test]
    async fn delete_nonexistent_store() {
        let provider = test_provider().await;
        let result = provider.delete_store("tenant-1", Uuid::new_v4()).await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn store_config_roundtrip() {
        let provider = test_provider().await;
        let config = StoreConfig {
            default_ttl_seconds: Some(3600),
            default_sharing_policy: Some("private".into()),
            embedding_model: Some("text-embedding-3-small".into()),
        };

        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "configured".into(),
                    region: None,
                    config: config.clone(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let fetched = provider.get_store("tenant-1", store.id).await.unwrap();
        assert_eq!(fetched.config.default_ttl_seconds, Some(3600));
        assert_eq!(
            fetched.config.embedding_model.as_deref(),
            Some("text-embedding-3-small")
        );
    }

    // --- Helper to create a store + provider for memory tests ---

    async fn setup_with_store() -> (SqliteProvider, kd6_core::models::MemoryStore) {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "test-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();
        (provider, store)
    }

    fn make_memory_request(agent: &str) -> CreateMemoryRequest {
        CreateMemoryRequest {
            layer: kd6_core::models::MemoryLayer::Working,
            content: serde_json::json!({"text": "hello world"}),
            embedding: Some(vec![1.0, 0.0, 0.0]),
            owner_agent_id: agent.into(),
            scope: kd6_core::models::MemoryScope {
                tenant_id: "tenant-1".into(),
                org_id: None,
                team_id: None,
                project_id: None,
                user_id: None,
                agent_id: Some(agent.into()),
                session_id: None,
                run_id: None,
            },
            tags: vec!["test".into()],
            categories: vec!["unit-test".into()],
            source: None,
            access_control: Default::default(),
            expires_at: None,
            immutable: false,
            valid_from: None,
            valid_until: None,
            confidence: None,
            entity_type: None,
            upsert_key: None,
        }
    }

    #[tokio::test]
    async fn create_and_get_memory() {
        let (provider, store) = setup_with_store().await;
        let entry = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
            .await
            .unwrap();

        assert_eq!(entry.version, 1);
        assert_eq!(entry.owner_agent_id, "agent-1");
        assert_eq!(entry.tags, vec!["test"]);
        assert_eq!(entry.embedding, Some(vec![1.0, 0.0, 0.0]));

        let fetched = provider
            .get_memory("tenant-1", store.id, entry.id)
            .await
            .unwrap();
        assert_eq!(fetched.id, entry.id);
        assert_eq!(fetched.content, serde_json::json!({"text": "hello world"}));
        assert_eq!(fetched.embedding, Some(vec![1.0, 0.0, 0.0]));
    }

    #[tokio::test]
    async fn get_memory_wrong_tenant() {
        let (provider, store) = setup_with_store().await;
        let entry = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
            .await
            .unwrap();

        let result = provider.get_memory("tenant-2", store.id, entry.id).await;
        assert!(matches!(result, Err(OmsError::MemoryNotFound(_))));
    }

    #[tokio::test]
    async fn update_memory_increments_version() {
        let (provider, store) = setup_with_store().await;
        let entry = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
            .await
            .unwrap();

        let updated = provider
            .update_memory(
                "tenant-1",
                store.id,
                entry.id,
                kd6_core::models::UpdateMemoryRequest {
                    content: Some(serde_json::json!({"text": "updated"})),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.version, 2);
        assert_eq!(updated.content, serde_json::json!({"text": "updated"}));
        assert!(updated.updated_at > entry.updated_at);
    }

    #[tokio::test]
    async fn test_optimistic_concurrency_conflict() {
        let provider = test_provider().await;
        let tenant = "t-occ";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "occ-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut request = make_memory_request("agent-1");
        request.scope.tenant_id = tenant.into();
        request.content = serde_json::json!({"text": "original"});
        let memory = provider
            .create_memory(tenant, store.id, request)
            .await
            .unwrap();
        assert_eq!(memory.version, 1);

        let updated = provider
            .update_memory(
                tenant,
                store.id,
                memory.id,
                UpdateMemoryRequest {
                    content: Some(serde_json::json!({"text": "updated"})),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.version, 2);

        let stale_content =
            serde_json::to_string(&serde_json::json!({"text": "stale write"})).unwrap();
        let stale_attempt = sqlx::query(
            "UPDATE memories SET content_json = ?, updated_at = ?, version = ?
             WHERE id = ? AND store_id = ? AND tenant_id = ? AND version = ?",
        )
        .bind(&stale_content)
        .bind(Utc::now().to_rfc3339())
        .bind(2)
        .bind(memory.id.to_string())
        .bind(store.id.to_string())
        .bind(tenant)
        .bind(1)
        .execute(&provider.pool)
        .await
        .unwrap();
        assert_eq!(stale_attempt.rows_affected(), 0);

        let updated2 = provider
            .update_memory(
                tenant,
                store.id,
                memory.id,
                UpdateMemoryRequest {
                    content: Some(serde_json::json!({"text": "updated again"})),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated2.version, 3);
    }

    #[tokio::test]
    async fn update_immutable_memory_fails() {
        let (provider, store) = setup_with_store().await;
        let mut req = make_memory_request("agent-1");
        req.immutable = true;

        let entry = provider
            .create_memory("tenant-1", store.id, req)
            .await
            .unwrap();

        let result = provider
            .update_memory(
                "tenant-1",
                store.id,
                entry.id,
                kd6_core::models::UpdateMemoryRequest {
                    content: Some(serde_json::json!("nope")),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await;

        assert!(matches!(result, Err(OmsError::Immutable(_))));
    }

    #[tokio::test]
    async fn delete_memory_success() {
        let (provider, store) = setup_with_store().await;
        let entry = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
            .await
            .unwrap();

        provider
            .delete_memory("tenant-1", store.id, entry.id)
            .await
            .unwrap();

        let result = provider.get_memory("tenant-1", store.id, entry.id).await;
        assert!(matches!(result, Err(OmsError::MemoryNotFound(_))));
    }

    #[tokio::test]
    async fn list_memories_pagination() {
        let (provider, store) = setup_with_store().await;

        for i in 0..5 {
            let mut req = make_memory_request("agent-1");
            req.content = serde_json::json!({"index": i});
            provider
                .create_memory("tenant-1", store.id, req)
                .await
                .unwrap();
        }

        let page1 = provider
            .list_memories(
                "tenant-1",
                store.id,
                ListMemoriesFilter {
                    limit: Some(2),
                    offset: Some(0),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(page1.items.len(), 2);
        assert_eq!(page1.total, 5);
        assert_eq!(page1.limit, 2);
        assert_eq!(page1.offset, 0);

        let page2 = provider
            .list_memories(
                "tenant-1",
                store.id,
                ListMemoriesFilter {
                    limit: Some(2),
                    offset: Some(2),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(page2.items.len(), 2);
        assert_eq!(page2.offset, 2);
    }

    #[tokio::test]
    async fn list_memories_filter_by_owner() {
        let (provider, store) = setup_with_store().await;
        provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-a"))
            .await
            .unwrap();
        provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-b"))
            .await
            .unwrap();
        provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-a"))
            .await
            .unwrap();

        let page = provider
            .list_memories(
                "tenant-1",
                store.id,
                ListMemoriesFilter {
                    owner_agent_id: Some("agent-a".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(page.total, 2);
        assert!(page.items.iter().all(|m| m.owner_agent_id == "agent-a"));
    }

    #[tokio::test]
    async fn embedding_roundtrip() {
        let embedding = vec![0.1, 0.2, 0.3, -0.5, 1.0];
        let bytes = super::embedding_to_bytes(&embedding);
        let recovered = super::bytes_to_embedding(&bytes);
        assert_eq!(embedding, recovered);
    }

    #[tokio::test]
    async fn search_by_vector_similarity() {
        let (provider, store) = setup_with_store().await;

        // Insert memories with different embeddings
        let mut req1 = make_memory_request("agent-1");
        req1.embedding = Some(vec![1.0, 0.0, 0.0]); // points along x-axis
        req1.content = serde_json::json!("x-axis");
        provider
            .create_memory("tenant-1", store.id, req1)
            .await
            .unwrap();

        let mut req2 = make_memory_request("agent-1");
        req2.embedding = Some(vec![0.0, 1.0, 0.0]); // points along y-axis
        req2.content = serde_json::json!("y-axis");
        provider
            .create_memory("tenant-1", store.id, req2)
            .await
            .unwrap();

        let mut req3 = make_memory_request("agent-1");
        req3.embedding = Some(vec![0.9, 0.1, 0.0]); // close to x-axis
        req3.content = serde_json::json!("near-x");
        provider
            .create_memory("tenant-1", store.id, req3)
            .await
            .unwrap();

        // Search for vectors similar to x-axis
        let results = provider
            .search(
                "tenant-1",
                store.id,
                SearchQuery {
                    query: "test".into(),
                    embedding: Some(vec![1.0, 0.0, 0.0]),
                    layers: vec![],
                    scope: None,
                    top_k: 2,
                    threshold: 0.5,
                    filters: Default::default(),
                    keyword: false,
                },
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        // First result should be exact match (score ~1.0)
        assert!(results[0].score > 0.99);
        assert_eq!(results[0].entry.content, serde_json::json!("x-axis"));
        // Second should be near-x (high similarity)
        assert!(results[1].score > 0.9);
        assert_eq!(results[1].entry.content, serde_json::json!("near-x"));
    }

    #[tokio::test]
    async fn search_with_threshold_filters() {
        let (provider, store) = setup_with_store().await;

        let mut req1 = make_memory_request("agent-1");
        req1.embedding = Some(vec![1.0, 0.0, 0.0]);
        provider
            .create_memory("tenant-1", store.id, req1)
            .await
            .unwrap();

        let mut req2 = make_memory_request("agent-1");
        req2.embedding = Some(vec![0.0, 1.0, 0.0]); // orthogonal, score ~0.0
        provider
            .create_memory("tenant-1", store.id, req2)
            .await
            .unwrap();

        // High threshold should exclude orthogonal vector
        let results = provider
            .search(
                "tenant-1",
                store.id,
                SearchQuery {
                    query: "test".into(),
                    embedding: Some(vec![1.0, 0.0, 0.0]),
                    layers: vec![],
                    scope: None,
                    top_k: 10,
                    threshold: 0.5,
                    filters: Default::default(),
                    keyword: false,
                },
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn search_requires_embedding() {
        let (provider, store) = setup_with_store().await;

        let result = provider
            .search(
                "tenant-1",
                store.id,
                SearchQuery {
                    query: "test".into(),
                    embedding: None,
                    layers: vec![],
                    scope: None,
                    top_k: 10,
                    threshold: 0.0,
                    filters: Default::default(),
                    keyword: false,
                },
            )
            .await;

        assert!(matches!(result, Err(OmsError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn audit_log_returns_memory_mutation_entries() {
        let (provider, store) = setup_with_store().await;
        let memory = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-a"))
            .await
            .unwrap();
        provider
            .update_memory(
                "tenant-1",
                store.id,
                memory.id,
                UpdateMemoryRequest {
                    content: Some(serde_json::json!({"text": "updated"})),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        provider
            .delete_memory("tenant-1", store.id, memory.id)
            .await
            .unwrap();

        let page = provider
            .audit_log(
                "tenant-1",
                store.id,
                kd6_core::models::AuditFilter {
                    memory_id: Some(memory.id),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(page.total, 3);
        let actions: Vec<String> = page
            .items
            .iter()
            .map(|entry| entry.action.clone())
            .collect();
        assert!(actions.contains(&"create".to_string()));
        assert!(actions.contains(&"update".to_string()));
        assert!(actions.contains(&"delete".to_string()));
    }

    #[tokio::test]
    async fn test_audit_hash_chain_integrity() {
        let provider = test_provider().await;
        let tenant = "t-audit-hash";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "audit-hash-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut req1 = make_memory_request("agent-audit-1");
        req1.scope.tenant_id = tenant.into();
        let m1 = provider
            .create_memory(tenant, store.id, req1)
            .await
            .unwrap();

        let mut req2 = make_memory_request("agent-audit-2");
        req2.scope.tenant_id = tenant.into();
        provider
            .create_memory(tenant, store.id, req2)
            .await
            .unwrap();

        provider
            .delete_memory(tenant, store.id, m1.id)
            .await
            .unwrap();

        let audit = provider
            .audit_log(tenant, store.id, kd6_core::models::AuditFilter::default())
            .await
            .unwrap();

        assert!(
            audit.items.len() >= 3,
            "expected at least 3 audit entries, got {}",
            audit.items.len()
        );

        let rows = sqlx::query(
            "SELECT entry_hash, prev_hash FROM audit_log WHERE store_id = ? AND tenant_id = ? ORDER BY rowid ASC",
        )
        .bind(store.id.to_string())
        .bind(tenant)
        .fetch_all(&provider.pool)
        .await
        .unwrap();

        assert!(rows.len() >= 3);

        let first_prev: Option<String> = rows[0].get("prev_hash");
        assert!(
            first_prev.is_none(),
            "first audit entry should have no prev_hash"
        );

        let first_hash: Option<String> = rows[0].get("entry_hash");
        assert!(
            first_hash.is_some(),
            "first audit entry should have entry_hash"
        );

        for i in 1..rows.len() {
            let prev_hash: Option<String> = rows[i].get("prev_hash");
            let prev_entry_hash: Option<String> = rows[i - 1].get("entry_hash");
            assert_eq!(prev_hash, prev_entry_hash, "hash chain broken at entry {i}");
        }
    }

    #[tokio::test]
    async fn purge_expired_removes_only_expired_entries() {
        let (provider, store) = setup_with_store().await;

        let mut expired = make_memory_request("agent-a");
        expired.expires_at = Some(Utc::now() - Duration::minutes(5));
        provider
            .create_memory("tenant-1", store.id, expired)
            .await
            .unwrap();

        let mut active = make_memory_request("agent-a");
        active.expires_at = Some(Utc::now() + Duration::minutes(5));
        provider
            .create_memory("tenant-1", store.id, active)
            .await
            .unwrap();

        let purged = provider.purge_expired("tenant-1", store.id).await.unwrap();
        assert_eq!(purged, 1);

        let page = provider
            .list_memories("tenant-1", store.id, ListMemoriesFilter::default())
            .await
            .unwrap();
        assert_eq!(page.total, 1);
    }

    #[tokio::test]
    async fn batch_create_memories_handles_partial_failures() {
        let (provider, store) = setup_with_store().await;
        sqlx::query(
            "CREATE TRIGGER block_bad_batch_insert BEFORE INSERT ON memories
             WHEN new.owner_agent_id = 'bad-agent' BEGIN
                 SELECT RAISE(ABORT, 'blocked owner');
             END;",
        )
        .execute(&provider.pool)
        .await
        .unwrap();

        let response = provider
            .batch_create_memories(
                "tenant-1",
                store.id,
                kd6_core::models::BatchCreateRequest {
                    entries: vec![
                        make_memory_request("good-agent"),
                        make_memory_request("bad-agent"),
                        make_memory_request("good-agent-2"),
                    ],
                },
            )
            .await
            .unwrap();

        assert_eq!(response.created.len(), 2);
        assert_eq!(response.errors.len(), 1);
        assert_eq!(response.errors[0].index, 1);
    }

    #[tokio::test]
    async fn inheritance_crud_roundtrip() {
        let (provider, store) = setup_with_store().await;
        let inheritance = provider
            .create_inheritance(
                "tenant-1",
                store.id,
                kd6_core::models::CreateInheritanceRequest {
                    parent_agent_id: "parent".into(),
                    child_agent_id: "child".into(),
                    inherit_layers: vec![MemoryLayer::Semantic],
                    filter: Default::default(),
                    access: kd6_core::models::InheritanceAccess::ReadOnly,
                    bubble_up: kd6_core::models::BubbleUpConfig {
                        enabled: true,
                        auto_summarize: false,
                        layers: vec![MemoryLayer::Semantic],
                    },
                },
            )
            .await
            .unwrap();

        let row = sqlx::query(
            "SELECT COUNT(*) AS cnt FROM inheritance WHERE id = ? AND store_id = ? AND tenant_id = ?",
        )
        .bind(inheritance.id.to_string())
        .bind(store.id.to_string())
        .bind("tenant-1")
        .fetch_one(&provider.pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("cnt"), 1);

        provider
            .delete_inheritance("tenant-1", store.id, inheritance.id)
            .await
            .unwrap();

        let row = sqlx::query(
            "SELECT COUNT(*) AS cnt FROM inheritance WHERE id = ? AND store_id = ? AND tenant_id = ?",
        )
        .bind(inheritance.id.to_string())
        .bind(store.id.to_string())
        .bind("tenant-1")
        .fetch_one(&provider.pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("cnt"), 0);
    }

    #[tokio::test]
    async fn shared_space_lifecycle() {
        let (provider, store) = setup_with_store().await;
        let space = provider
            .create_shared_space(
                "tenant-1",
                store.id,
                kd6_core::models::CreateSharedSpaceRequest {
                    name: "blackboard".into(),
                    scope: MemoryScope {
                        tenant_id: "tenant-1".into(),
                        org_id: None,
                        team_id: None,
                        project_id: None,
                        user_id: None,
                        agent_id: None,
                        session_id: None,
                        run_id: None,
                    },
                    layer: MemoryLayer::Working,
                    conflict_resolution: kd6_core::models::ConflictResolution::LastWriteWins,
                    notify_on_write: true,
                    notify_on_delete: false,
                },
            )
            .await
            .unwrap();
        assert!(space.participants.is_empty());

        let joined = provider
            .join_shared_space(
                "tenant-1",
                store.id,
                space.id,
                kd6_core::models::JoinSpaceRequest {
                    agent_id: "agent-1".into(),
                    access: kd6_core::models::ParticipantAccess::ReadWrite,
                },
            )
            .await
            .unwrap();
        assert_eq!(joined.participants.len(), 1);
        assert_eq!(joined.participants[0].agent_id, "agent-1");

        let listed = provider
            .list_shared_spaces("tenant-1", store.id)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].participants.len(), 1);

        provider
            .leave_shared_space(
                "tenant-1",
                store.id,
                space.id,
                kd6_core::models::LeaveSpaceRequest {
                    agent_id: "agent-1".into(),
                },
            )
            .await
            .unwrap();
        let fetched = provider
            .get_shared_space("tenant-1", store.id, space.id)
            .await
            .unwrap();
        assert!(fetched.participants.is_empty());
    }

    #[tokio::test]
    async fn search_supports_keyword_fts() {
        let (provider, store) = setup_with_store().await;

        let mut target = make_memory_request("agent-1");
        target.embedding = None;
        target.content = serde_json::json!({"text": "alpha keyword match"});
        provider
            .create_memory("tenant-1", store.id, target)
            .await
            .unwrap();

        let mut other = make_memory_request("agent-2");
        other.embedding = None;
        other.content = serde_json::json!({"text": "completely different"});
        provider
            .create_memory("tenant-1", store.id, other)
            .await
            .unwrap();

        let results = provider
            .search(
                "tenant-1",
                store.id,
                SearchQuery {
                    query: "alpha".into(),
                    embedding: None,
                    layers: vec![],
                    scope: None,
                    top_k: 10,
                    threshold: 0.0,
                    filters: Default::default(),
                    keyword: true,
                },
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].entry.content,
            serde_json::json!({"text": "alpha keyword match"})
        );
    }

    #[tokio::test]
    async fn stats_returns_counts() {
        let (provider, store) = setup_with_store().await;

        for _ in 0..3 {
            provider
                .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
                .await
                .unwrap();
        }

        let stats = provider.stats("tenant-1", store.id).await.unwrap();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.entries_by_layer.get(&MemoryLayer::Working), Some(&3));
    }

    #[test]
    fn cosine_similarity_unit() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((super::cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!(super::cosine_similarity(&a, &c).abs() < 1e-6);

        let d = vec![-1.0, 0.0, 0.0];
        assert!((super::cosine_similarity(&a, &d) + 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_graph_create_and_traverse() {
        let provider = test_provider().await;
        let tenant = "t-graph";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "graph-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut m1_req = make_memory_request("agent-1");
        m1_req.scope.tenant_id = tenant.into();
        m1_req.content = serde_json::json!({"text": "Node A"});
        let m1 = provider
            .create_memory(tenant, store.id, m1_req)
            .await
            .unwrap();

        let mut m2_req = make_memory_request("agent-1");
        m2_req.scope.tenant_id = tenant.into();
        m2_req.content = serde_json::json!({"text": "Node B"});
        let m2 = provider
            .create_memory(tenant, store.id, m2_req)
            .await
            .unwrap();

        let mut m3_req = make_memory_request("agent-1");
        m3_req.scope.tenant_id = tenant.into();
        m3_req.content = serde_json::json!({"text": "Node C"});
        let m3 = provider
            .create_memory(tenant, store.id, m3_req)
            .await
            .unwrap();

        let edge1 = provider
            .create_edge(
                tenant,
                store.id,
                kd6_core::models::CreateEdgeRequest {
                    source_memory_id: m1.id,
                    target_memory_id: m2.id,
                    relation_type: "related_to".into(),
                    weight: 1.0,
                    metadata: serde_json::json!({}),
                },
            )
            .await
            .unwrap();
        assert_eq!(edge1.source_memory_id, m1.id);
        assert_eq!(edge1.target_memory_id, m2.id);

        let _edge2 = provider
            .create_edge(
                tenant,
                store.id,
                kd6_core::models::CreateEdgeRequest {
                    source_memory_id: m2.id,
                    target_memory_id: m3.id,
                    relation_type: "depends_on".into(),
                    weight: 0.8,
                    metadata: serde_json::json!({}),
                },
            )
            .await
            .unwrap();

        let result = provider
            .graph_traverse(
                tenant,
                store.id,
                kd6_core::models::GraphTraversalRequest {
                    start_memory_id: m1.id,
                    depth: 2,
                    relation_types: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.nodes.len(), 3);
        assert_eq!(result.edges.len(), 2);

        let result = provider
            .graph_traverse(
                tenant,
                store.id,
                kd6_core::models::GraphTraversalRequest {
                    start_memory_id: m1.id,
                    depth: 1,
                    relation_types: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);

        let result = provider
            .graph_traverse(
                tenant,
                store.id,
                kd6_core::models::GraphTraversalRequest {
                    start_memory_id: m1.id,
                    depth: 2,
                    relation_types: Some(vec!["related_to".into()]),
                },
            )
            .await
            .unwrap();
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);
    }

    #[tokio::test]
    async fn test_graph_delete_edge() {
        let provider = test_provider().await;
        let tenant = "t-graph-del";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "graph-del-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut m1_req = make_memory_request("agent-1");
        m1_req.scope.tenant_id = tenant.into();
        m1_req.content = serde_json::json!({"text": "X"});
        let m1 = provider
            .create_memory(tenant, store.id, m1_req)
            .await
            .unwrap();

        let mut m2_req = make_memory_request("agent-1");
        m2_req.scope.tenant_id = tenant.into();
        m2_req.content = serde_json::json!({"text": "Y"});
        let m2 = provider
            .create_memory(tenant, store.id, m2_req)
            .await
            .unwrap();

        let edge = provider
            .create_edge(
                tenant,
                store.id,
                kd6_core::models::CreateEdgeRequest {
                    source_memory_id: m1.id,
                    target_memory_id: m2.id,
                    relation_type: "test".into(),
                    weight: 1.0,
                    metadata: serde_json::json!({}),
                },
            )
            .await
            .unwrap();

        provider
            .delete_edge(tenant, store.id, edge.id)
            .await
            .unwrap();
        let err = provider.delete_edge(tenant, store.id, edge.id).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_gdpr_purge() {
        let provider = test_provider().await;
        let tenant = "t-gdpr";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "gdpr-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut req1 = make_memory_request("agent-1");
        req1.scope.tenant_id = tenant.into();
        req1.scope.user_id = Some("user-a".into());
        req1.content = serde_json::json!({"text": "User A data"});
        let mut req2 = make_memory_request("agent-1");
        req2.scope.tenant_id = tenant.into();
        req2.scope.user_id = Some("user-a".into());
        req2.content = serde_json::json!({"text": "User A more data"});
        let mut req3 = make_memory_request("agent-1");
        req3.scope.tenant_id = tenant.into();
        req3.scope.user_id = Some("user-b".into());
        req3.content = serde_json::json!({"text": "User B data"});

        provider
            .create_memory(tenant, store.id, req1)
            .await
            .unwrap();
        provider
            .create_memory(tenant, store.id, req2)
            .await
            .unwrap();
        provider
            .create_memory(tenant, store.id, req3)
            .await
            .unwrap();

        let deleted = provider
            .gdpr_purge(
                tenant,
                store.id,
                kd6_core::models::MemoryScope {
                    tenant_id: tenant.into(),
                    org_id: None,
                    team_id: None,
                    project_id: None,
                    user_id: Some("user-a".into()),
                    agent_id: None,
                    session_id: None,
                    run_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(deleted, 2);

        let remaining = provider
            .list_memories(tenant, store.id, ListMemoriesFilter::default())
            .await
            .unwrap();
        assert_eq!(remaining.items.len(), 1);
    }

    #[tokio::test]
    async fn test_gdpr_purge_rejects_empty_scope() {
        let provider = test_provider().await;
        let tenant = "t-gdpr-empty";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "gdpr-guard-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let result = provider
            .gdpr_purge(tenant, store.id, MemoryScope::default())
            .await;
        assert!(result.is_err(), "empty scope should be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("at least one scope field"),
            "error should mention scope requirement: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_temporal_metadata() {
        let provider = test_provider().await;
        let tenant = "t-temporal";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "temporal-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let now = chrono::Utc::now();
        let future = now + chrono::Duration::hours(24);

        let mut req = make_memory_request("agent-1");
        req.scope.tenant_id = tenant.into();
        req.content = serde_json::json!({"text": "temporal fact"});
        req.valid_from = Some(now);
        req.valid_until = Some(future);
        req.confidence = Some(0.95);
        req.entity_type = Some("fact".into());

        let entry = provider.create_memory(tenant, store.id, req).await.unwrap();
        assert!(entry.valid_from.is_some());
        assert!(entry.valid_until.is_some());
        assert_eq!(entry.confidence, Some(0.95));
        assert_eq!(entry.entity_type.as_deref(), Some("fact"));

        let fetched = provider
            .get_memory(tenant, store.id, entry.id)
            .await
            .unwrap();
        assert!(fetched.valid_from.is_some());
        assert!(fetched.valid_until.is_some());
        assert_eq!(fetched.confidence, Some(0.95));
        assert_eq!(fetched.entity_type.as_deref(), Some("fact"));
    }

    #[tokio::test]
    async fn test_sovereignty_config() {
        let provider = test_provider().await;
        let tenant = "t-sovereignty";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "sovereign-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        assert_eq!(
            store.sovereignty.mode,
            kd6_core::models::sovereignty::SovereigntyMode::Any
        );
    }

    #[tokio::test]
    async fn upsert_same_key_same_scope_updates_in_place() {
        let (provider, store) = setup_with_store().await;
        let mut req = make_memory_request("agent-1");
        req.upsert_key = Some("dedup-1".into());
        req.content = serde_json::json!({"text": "first version"});

        let first = provider
            .create_memory("tenant-1", store.id, req.clone())
            .await
            .unwrap();
        assert_eq!(first.version, 1);

        // Second create with same key should upsert (update in place)
        req.content = serde_json::json!({"text": "second version"});
        let second = provider
            .create_memory("tenant-1", store.id, req)
            .await
            .unwrap();

        assert_eq!(second.id, first.id, "should reuse same ID");
        assert_eq!(second.version, 2);
        assert_eq!(
            second.content,
            serde_json::json!({"text": "second version"})
        );
        // created_at should be preserved from the original insert
        assert_eq!(second.created_at, first.created_at);
    }

    #[tokio::test]
    async fn upsert_same_key_different_scope_creates_distinct_entries() {
        let (provider, store) = setup_with_store().await;
        let mut req1 = make_memory_request("agent-1");
        req1.upsert_key = Some("dedup-2".into());
        req1.scope.user_id = Some("user-a".into());
        req1.content = serde_json::json!({"text": "user A data"});

        let mut req2 = make_memory_request("agent-1");
        req2.upsert_key = Some("dedup-2".into());
        req2.scope.user_id = Some("user-b".into());
        req2.content = serde_json::json!({"text": "user B data"});

        let entry_a = provider
            .create_memory("tenant-1", store.id, req1)
            .await
            .unwrap();
        let entry_b = provider
            .create_memory("tenant-1", store.id, req2)
            .await
            .unwrap();

        assert_ne!(
            entry_a.id, entry_b.id,
            "different scopes should create distinct entries"
        );
        assert_eq!(entry_a.scope.user_id, Some("user-a".into()));
        assert_eq!(entry_b.scope.user_id, Some("user-b".into()));
    }

    #[tokio::test]
    async fn upsert_persists_scope_columns() {
        let (provider, store) = setup_with_store().await;
        let mut req = make_memory_request("agent-1");
        req.upsert_key = Some("scope-test".into());
        req.scope.user_id = Some("user-x".into());
        req.scope.team_id = Some("team-y".into());

        provider
            .create_memory("tenant-1", store.id, req.clone())
            .await
            .unwrap();

        // Upsert with same scope
        req.content = serde_json::json!({"text": "updated"});
        let updated = provider
            .create_memory("tenant-1", store.id, req)
            .await
            .unwrap();

        // Fetch from DB to verify scope is actually persisted (not just request echo)
        let fetched = provider
            .get_memory("tenant-1", store.id, updated.id)
            .await
            .unwrap();
        assert_eq!(fetched.scope.user_id, Some("user-x".into()));
        assert_eq!(fetched.scope.team_id, Some("team-y".into()));
        assert_eq!(fetched.content, serde_json::json!({"text": "updated"}));
    }

    #[tokio::test]
    async fn gdpr_purge_sets_redacted_flag_on_audit_entries() {
        let provider = test_provider().await;
        let tenant = "t-gdpr-redact";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "redact-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut req = make_memory_request("agent-1");
        req.scope.tenant_id = tenant.into();
        req.scope.user_id = Some("user-purge".into());
        let entry = provider.create_memory(tenant, store.id, req).await.unwrap();

        // There should be audit entries for the create
        let audit_before = provider
            .audit_log(
                tenant,
                store.id,
                AuditFilter {
                    memory_id: Some(entry.id),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(!audit_before.items.is_empty());
        assert!(!audit_before.items[0].redacted);

        // Purge user-purge's data
        provider
            .gdpr_purge(
                tenant,
                store.id,
                kd6_core::models::MemoryScope {
                    tenant_id: tenant.into(),
                    user_id: Some("user-purge".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        // Audit entries for the purged memory should be redacted
        let audit_after = provider
            .audit_log(
                tenant,
                store.id,
                AuditFilter {
                    memory_id: Some(entry.id),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(!audit_after.items.is_empty());
        for audit_entry in &audit_after.items {
            if audit_entry.action != "gdpr_purge" {
                assert!(
                    audit_entry.redacted,
                    "audit entry should be marked redacted"
                );
                assert!(
                    audit_entry.agent_id.is_none(),
                    "agent_id should be anonymized"
                );
                assert!(
                    audit_entry.details.is_none(),
                    "details should be anonymized"
                );
            }
        }
    }

    #[tokio::test]
    async fn get_store_by_name_returns_store() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "named-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let found = provider
            .get_store_by_name("tenant-1", "named-store")
            .await
            .unwrap();
        assert_eq!(found.id, store.id);
        assert_eq!(found.name, "named-store");
    }

    #[tokio::test]
    async fn get_store_by_name_not_found() {
        let provider = test_provider().await;
        let result = provider.get_store_by_name("tenant-1", "nonexistent").await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn get_store_by_name_tenant_isolation() {
        let provider = test_provider().await;
        provider
            .create_store(
                "tenant-a",
                CreateStoreRequest {
                    name: "shared-name".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        // Same name, different tenant — should not be found
        let result = provider.get_store_by_name("tenant-b", "shared-name").await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn same_name_different_tenants_allowed() {
        let provider = test_provider().await;
        let a = provider
            .create_store(
                "tenant-a",
                CreateStoreRequest {
                    name: "my-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();
        let b = provider
            .create_store(
                "tenant-b",
                CreateStoreRequest {
                    name: "my-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        assert_ne!(a.id, b.id);
        assert_eq!(a.name, b.name);
    }

    #[tokio::test]
    async fn duplicate_name_same_tenant_rejected() {
        let provider = test_provider().await;
        provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "unique-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let result = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "unique-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await;
        assert!(result.is_err(), "duplicate name in same tenant should fail");
    }
}

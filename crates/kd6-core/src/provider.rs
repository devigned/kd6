use async_trait::async_trait;
use uuid::Uuid;

use crate::error::OmsError;
use crate::models::{
    AuditEntry, AuditFilter, BatchCreateRequest, BatchCreateResponse, BatchDeleteRequest,
    BatchDeleteResponse, BubbleUpRequest, CreateEdgeRequest, CreateInheritanceRequest,
    CreateMemoryRequest, CreateSharedSpaceRequest, CreateStoreRequest, GraphEdge,
    GraphTraversalRequest, GraphTraversalResult, InheritanceSpec, JoinSpaceRequest,
    LeaveSpaceRequest, ListMemoriesFilter, MemoryEntry, MemoryStore, Page, ProviderCapabilities,
    SearchQuery, SearchResult, SharedSpace, StoreStats, UpdateMemoryRequest, UpdateStoreRequest,
};

/// The core service provider interface (SPI) for OMS backends.
///
/// Implementations of this trait provide the storage and retrieval logic for
/// memory stores and entries. The platform calls these methods after
/// authentication and tenant validation.
#[async_trait]
pub trait OmsProvider: Send + Sync {
    // --- Store Management ---

    async fn create_store(
        &self,
        tenant_id: &str,
        request: CreateStoreRequest,
    ) -> Result<MemoryStore, OmsError>;

    async fn get_store(&self, tenant_id: &str, store_id: Uuid) -> Result<MemoryStore, OmsError>;

    async fn list_stores(&self, tenant_id: &str) -> Result<Vec<MemoryStore>, OmsError>;

    async fn update_store(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: UpdateStoreRequest,
    ) -> Result<MemoryStore, OmsError>;

    async fn delete_store(&self, tenant_id: &str, store_id: Uuid) -> Result<(), OmsError>;

    /// Atomically get an existing store by name, or create it if it doesn't exist.
    /// Used for `_default` store auto-provisioning (OMS spec 4.1.1).
    async fn get_or_create_store(
        &self,
        tenant_id: &str,
        name: &str,
        request: CreateStoreRequest,
    ) -> Result<MemoryStore, OmsError> {
        // Default implementation: list + create (non-atomic, override for atomicity)
        let stores = self.list_stores(tenant_id).await?;
        if let Some(store) = stores.into_iter().find(|s| s.name == name) {
            return Ok(store);
        }
        self.create_store(tenant_id, request).await
    }

    // --- Memory CRUD ---

    async fn create_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateMemoryRequest,
    ) -> Result<MemoryEntry, OmsError>;

    async fn get_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Uuid,
    ) -> Result<MemoryEntry, OmsError>;

    async fn list_memories(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        filter: ListMemoriesFilter,
    ) -> Result<Page<MemoryEntry>, OmsError>;

    async fn update_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Uuid,
        request: UpdateMemoryRequest,
    ) -> Result<MemoryEntry, OmsError>;

    async fn delete_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Uuid,
    ) -> Result<(), OmsError>;

    // --- Search ---

    async fn search(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        query: SearchQuery,
    ) -> Result<Vec<SearchResult>, OmsError>;

    // --- Health & Capabilities ---

    async fn stats(&self, tenant_id: &str, store_id: Uuid) -> Result<StoreStats, OmsError>;

    fn capabilities(&self) -> ProviderCapabilities;

    // --- Level 2: Audit ---

    async fn audit_log(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        filter: AuditFilter,
    ) -> Result<Page<AuditEntry>, OmsError>;

    // --- Level 2: Lifecycle ---

    async fn purge_expired(&self, tenant_id: &str, store_id: Uuid) -> Result<u64, OmsError>;

    // --- Level 2: Batch ---

    async fn batch_create_memories(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: BatchCreateRequest,
    ) -> Result<BatchCreateResponse, OmsError>;

    async fn batch_delete_memories(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: BatchDeleteRequest,
    ) -> Result<BatchDeleteResponse, OmsError>;

    // --- Level 2: Inheritance ---

    async fn create_inheritance(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateInheritanceRequest,
    ) -> Result<InheritanceSpec, OmsError>;

    async fn delete_inheritance(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        inheritance_id: Uuid,
    ) -> Result<(), OmsError>;

    async fn bubble_up(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: BubbleUpRequest,
    ) -> Result<Vec<MemoryEntry>, OmsError>;

    // --- Level 2: Shared Spaces ---

    async fn create_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateSharedSpaceRequest,
    ) -> Result<SharedSpace, OmsError>;

    async fn list_shared_spaces(
        &self,
        tenant_id: &str,
        store_id: Uuid,
    ) -> Result<Vec<SharedSpace>, OmsError>;

    async fn get_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
    ) -> Result<SharedSpace, OmsError>;

    async fn join_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
        request: JoinSpaceRequest,
    ) -> Result<SharedSpace, OmsError>;

    async fn leave_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
        request: LeaveSpaceRequest,
    ) -> Result<(), OmsError>;

    async fn delete_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
    ) -> Result<(), OmsError>;

    // --- Level 3: Graph ---

    async fn create_edge(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateEdgeRequest,
    ) -> Result<GraphEdge, OmsError>;

    async fn delete_edge(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        edge_id: Uuid,
    ) -> Result<(), OmsError>;

    async fn graph_traverse(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: GraphTraversalRequest,
    ) -> Result<GraphTraversalResult, OmsError>;

    // --- Level 3: GDPR Purge ---

    async fn gdpr_purge(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        scope: crate::models::MemoryScope,
    ) -> Result<u64, OmsError>;
}

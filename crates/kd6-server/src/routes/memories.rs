use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use kd6_core::models::{
    CreateMemoryRequest, ListMemoriesFilter, MemoryEntry, Page, UpdateMemoryRequest,
};

use crate::embed::{auto_embed_content, auto_embed_update};
use crate::error::ApiError;
use crate::extract::{resolve_store, JsonBody, PathId, StoreRef, TenantId};
use crate::state::AppState;

pub async fn create_memory(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(mut request): JsonBody<CreateMemoryRequest>,
) -> Result<(StatusCode, Json<MemoryEntry>), ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;

    // Auto-embed if no embedding provided (OMS spec section 8.4.1)
    request.embedding =
        auto_embed_content(&*state.embedder, &request.content, request.embedding).await?;

    let entry = state
        .provider
        .create_memory(&tenant, store_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

pub async fn get_memory(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_ref, memory_id)): PathId<(StoreRef, Uuid)>,
) -> Result<Json<MemoryEntry>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let entry = state
        .provider
        .get_memory(&tenant, store_id, memory_id)
        .await?;
    Ok(Json(entry))
}

pub async fn list_memories(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    Query(filter): Query<ListMemoriesFilter>,
) -> Result<Json<Page<MemoryEntry>>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let page = state
        .provider
        .list_memories(&tenant, store_id, filter)
        .await?;
    Ok(Json(page))
}

pub async fn update_memory(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_ref, memory_id)): PathId<(StoreRef, Uuid)>,
    JsonBody(mut request): JsonBody<UpdateMemoryRequest>,
) -> Result<Json<MemoryEntry>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;

    // Auto-embed if content changed but no new embedding provided (OMS spec section 8.4.2)
    request.embedding = auto_embed_update(
        &*state.embedder,
        request.content.as_ref(),
        request.embedding,
    )
    .await?;

    let entry = state
        .provider
        .update_memory(&tenant, store_id, memory_id, request)
        .await?;
    Ok(Json(entry))
}

pub async fn delete_memory(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_ref, memory_id)): PathId<(StoreRef, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    state
        .provider
        .delete_memory(&tenant, store_id, memory_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use kd6_core::models::{
    CreateMemoryRequest, ListMemoriesFilter, MemoryEntry, Page, UpdateMemoryRequest,
};

use crate::error::ApiError;
use crate::extract::{JsonBody, PathId, TenantId};
use crate::state::AppState;

pub async fn create_memory(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    JsonBody(request): JsonBody<CreateMemoryRequest>,
) -> Result<(StatusCode, Json<MemoryEntry>), ApiError> {
    let entry = state
        .provider
        .create_memory(&tenant, store_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

pub async fn get_memory(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_id, memory_id)): PathId<(Uuid, Uuid)>,
) -> Result<Json<MemoryEntry>, ApiError> {
    let entry = state
        .provider
        .get_memory(&tenant, store_id, memory_id)
        .await?;
    Ok(Json(entry))
}

pub async fn list_memories(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    Query(filter): Query<ListMemoriesFilter>,
) -> Result<Json<Page<MemoryEntry>>, ApiError> {
    let page = state
        .provider
        .list_memories(&tenant, store_id, filter)
        .await?;
    Ok(Json(page))
}

pub async fn update_memory(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_id, memory_id)): PathId<(Uuid, Uuid)>,
    JsonBody(request): JsonBody<UpdateMemoryRequest>,
) -> Result<Json<MemoryEntry>, ApiError> {
    let entry = state
        .provider
        .update_memory(&tenant, store_id, memory_id, request)
        .await?;
    Ok(Json(entry))
}

pub async fn delete_memory(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_id, memory_id)): PathId<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .provider
        .delete_memory(&tenant, store_id, memory_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

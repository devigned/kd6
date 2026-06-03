use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use kd6_core::models::{BubbleUpRequest, CreateInheritanceRequest, InheritanceSpec, MemoryEntry};

use crate::error::ApiError;
use crate::extract::{resolve_store, JsonBody, PathId, StoreRef, TenantId};
use crate::state::AppState;

pub async fn create_inheritance(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(request): JsonBody<CreateInheritanceRequest>,
) -> Result<(StatusCode, Json<InheritanceSpec>), ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let spec = state
        .provider
        .create_inheritance(&tenant, store_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(spec)))
}

pub async fn delete_inheritance(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_ref, inheritance_id)): PathId<(StoreRef, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    state
        .provider
        .delete_inheritance(&tenant, store_id, inheritance_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn bubble_up(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(request): JsonBody<BubbleUpRequest>,
) -> Result<Json<Vec<MemoryEntry>>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let entries = state.provider.bubble_up(&tenant, store_id, request).await?;
    Ok(Json(entries))
}

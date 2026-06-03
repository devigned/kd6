use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use kd6_core::models::{CreateStoreRequest, MemoryStore, UpdateStoreRequest};

use crate::error::ApiError;
use crate::extract::{resolve_store, JsonBody, PathId, StoreRef, TenantId};
use crate::state::AppState;

pub async fn create_store(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    JsonBody(request): JsonBody<CreateStoreRequest>,
) -> Result<(StatusCode, Json<MemoryStore>), ApiError> {
    let store = state.provider.create_store(&tenant, request).await?;
    Ok((StatusCode::CREATED, Json(store)))
}

pub async fn get_store(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
) -> Result<Json<MemoryStore>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let store = state.provider.get_store(&tenant, store_id).await?;
    Ok(Json(store))
}

pub async fn list_stores(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
) -> Result<Json<Vec<MemoryStore>>, ApiError> {
    let stores = state.provider.list_stores(&tenant).await?;
    Ok(Json(stores))
}

pub async fn update_store(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(request): JsonBody<UpdateStoreRequest>,
) -> Result<Json<MemoryStore>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let store = state
        .provider
        .update_store(&tenant, store_id, request)
        .await?;
    Ok(Json(store))
}

pub async fn delete_store(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
) -> Result<StatusCode, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    state.provider.delete_store(&tenant, store_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

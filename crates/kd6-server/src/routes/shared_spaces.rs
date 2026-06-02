use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use kd6_core::models::{
    CreateSharedSpaceRequest, JoinSpaceRequest, LeaveSpaceRequest, SharedSpace,
};

use crate::error::ApiError;
use crate::extract::{JsonBody, PathId, TenantId};
use crate::state::AppState;

pub async fn create_shared_space(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    JsonBody(request): JsonBody<CreateSharedSpaceRequest>,
) -> Result<(StatusCode, Json<SharedSpace>), ApiError> {
    let space = state
        .provider
        .create_shared_space(&tenant, store_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(space)))
}

pub async fn list_shared_spaces(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
) -> Result<Json<Vec<SharedSpace>>, ApiError> {
    let spaces = state.provider.list_shared_spaces(&tenant, store_id).await?;
    Ok(Json(spaces))
}

pub async fn get_shared_space(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_id, space_id)): PathId<(Uuid, Uuid)>,
) -> Result<Json<SharedSpace>, ApiError> {
    let space = state
        .provider
        .get_shared_space(&tenant, store_id, space_id)
        .await?;
    Ok(Json(space))
}

pub async fn join_shared_space(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_id, space_id)): PathId<(Uuid, Uuid)>,
    JsonBody(request): JsonBody<JoinSpaceRequest>,
) -> Result<Json<SharedSpace>, ApiError> {
    let space = state
        .provider
        .join_shared_space(&tenant, store_id, space_id, request)
        .await?;
    Ok(Json(space))
}

pub async fn leave_shared_space(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_id, space_id)): PathId<(Uuid, Uuid)>,
    JsonBody(request): JsonBody<LeaveSpaceRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .provider
        .leave_shared_space(&tenant, store_id, space_id, request)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_shared_space(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_id, space_id)): PathId<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .provider
        .delete_shared_space(&tenant, store_id, space_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

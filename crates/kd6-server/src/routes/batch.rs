use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use kd6_core::models::{
    BatchCreateRequest, BatchCreateResponse, BatchDeleteRequest, BatchDeleteResponse,
};

use crate::error::ApiError;
use crate::extract::{JsonBody, PathId, TenantId};
use crate::state::AppState;

pub async fn batch_create(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    JsonBody(request): JsonBody<BatchCreateRequest>,
) -> Result<(StatusCode, Json<BatchCreateResponse>), ApiError> {
    let response = state
        .provider
        .batch_create_memories(&tenant, store_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn batch_delete(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    JsonBody(request): JsonBody<BatchDeleteRequest>,
) -> Result<Json<BatchDeleteResponse>, ApiError> {
    let response = state
        .provider
        .batch_delete_memories(&tenant, store_id, request)
        .await?;
    Ok(Json(response))
}

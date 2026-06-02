use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use kd6_core::models::{
    CreateEdgeRequest, GraphEdge, GraphTraversalRequest, GraphTraversalResult,
};

use crate::error::ApiError;
use crate::extract::{JsonBody, PathId, TenantId};
use crate::state::AppState;

pub async fn create_edge(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    JsonBody(request): JsonBody<CreateEdgeRequest>,
) -> Result<(StatusCode, Json<GraphEdge>), ApiError> {
    let edge = state
        .provider
        .create_edge(&tenant, store_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(edge)))
}

pub async fn delete_edge(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_id, edge_id)): PathId<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .provider
        .delete_edge(&tenant, store_id, edge_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn traverse(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    JsonBody(request): JsonBody<GraphTraversalRequest>,
) -> Result<Json<GraphTraversalResult>, ApiError> {
    let result = state
        .provider
        .graph_traverse(&tenant, store_id, request)
        .await?;
    Ok(Json(result))
}

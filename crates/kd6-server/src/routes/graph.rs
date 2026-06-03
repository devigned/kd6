use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use kd6_core::models::{CreateEdgeRequest, GraphEdge, GraphTraversalRequest, GraphTraversalResult};

use crate::error::ApiError;
use crate::extract::{resolve_store, JsonBody, PathId, StoreRef, TenantId};
use crate::state::AppState;

pub async fn create_edge(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(request): JsonBody<CreateEdgeRequest>,
) -> Result<(StatusCode, Json<GraphEdge>), ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let edge = state
        .provider
        .create_edge(&tenant, store_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(edge)))
}

pub async fn delete_edge(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_ref, edge_id)): PathId<(StoreRef, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    state
        .provider
        .delete_edge(&tenant, store_id, edge_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn traverse(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(request): JsonBody<GraphTraversalRequest>,
) -> Result<Json<GraphTraversalResult>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let result = state
        .provider
        .graph_traverse(&tenant, store_id, request)
        .await?;
    Ok(Json(result))
}

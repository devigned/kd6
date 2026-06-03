use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use kd6_core::models::{
    BatchCreateRequest, BatchCreateResponse, BatchDeleteRequest, BatchDeleteResponse,
};

use crate::embed::auto_embed_content;
use crate::error::ApiError;
use crate::extract::{resolve_store, JsonBody, PathId, StoreRef, TenantId};
use crate::state::AppState;

pub async fn batch_create(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(mut request): JsonBody<BatchCreateRequest>,
) -> Result<(StatusCode, Json<BatchCreateResponse>), ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;

    // Auto-embed each entry in the batch (OMS spec section 8.4.1)
    for entry in &mut request.entries {
        entry.embedding =
            auto_embed_content(&*state.embedder, &entry.content, entry.embedding.take()).await?;
    }

    let response = state
        .provider
        .batch_create_memories(&tenant, store_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn batch_delete(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(request): JsonBody<BatchDeleteRequest>,
) -> Result<Json<BatchDeleteResponse>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let response = state
        .provider
        .batch_delete_memories(&tenant, store_id, request)
        .await?;
    Ok(Json(response))
}

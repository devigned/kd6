use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::error::ApiError;
use crate::extract::{resolve_store, PathId, StoreRef, TenantId};
use crate::state::AppState;

#[derive(Serialize)]
pub struct PurgeResponse {
    pub deleted: u64,
}

pub async fn purge_expired(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
) -> Result<Json<PurgeResponse>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let deleted = state.provider.purge_expired(&tenant, store_id).await?;
    Ok(Json(PurgeResponse { deleted }))
}

pub async fn lifecycle_stats(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
) -> Result<Json<kd6_core::models::StoreStats>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let stats = state.provider.stats(&tenant, store_id).await?;
    Ok(Json(stats))
}

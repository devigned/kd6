use axum::extract::State;
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::{PathId, TenantId};
use crate::state::AppState;

#[derive(Serialize)]
pub struct PurgeResponse {
    pub deleted: u64,
}

pub async fn purge_expired(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
) -> Result<Json<PurgeResponse>, ApiError> {
    let deleted = state.provider.purge_expired(&tenant, store_id).await?;
    Ok(Json(PurgeResponse { deleted }))
}

pub async fn lifecycle_stats(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
) -> Result<Json<kd6_core::models::StoreStats>, ApiError> {
    let stats = state.provider.stats(&tenant, store_id).await?;
    Ok(Json(stats))
}

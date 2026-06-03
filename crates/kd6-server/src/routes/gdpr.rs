use axum::extract::State;
use axum::Json;
use serde::Serialize;

use kd6_core::models::MemoryScope;

use crate::error::ApiError;
use crate::extract::{resolve_store, JsonBody, PathId, StoreRef, TenantId};
use crate::state::AppState;

#[derive(Serialize)]
pub struct GdprPurgeResponse {
    pub deleted: u64,
}

pub async fn gdpr_purge(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(scope): JsonBody<MemoryScope>,
) -> Result<Json<GdprPurgeResponse>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;
    let deleted = state.provider.gdpr_purge(&tenant, store_id, scope).await?;
    Ok(Json(GdprPurgeResponse { deleted }))
}

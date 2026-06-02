use axum::extract::State;
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

use kd6_core::models::MemoryScope;

use crate::error::ApiError;
use crate::extract::{JsonBody, PathId, TenantId};
use crate::state::AppState;

#[derive(Serialize)]
pub struct GdprPurgeResponse {
    pub deleted: u64,
}

pub async fn gdpr_purge(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    JsonBody(scope): JsonBody<MemoryScope>,
) -> Result<Json<GdprPurgeResponse>, ApiError> {
    let deleted = state.provider.gdpr_purge(&tenant, store_id, scope).await?;
    Ok(Json(GdprPurgeResponse { deleted }))
}

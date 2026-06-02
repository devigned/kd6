use axum::extract::{Query, State};
use axum::Json;
use uuid::Uuid;

use kd6_core::models::{AuditEntry, AuditFilter, Page};

use crate::error::ApiError;
use crate::extract::{PathId, TenantId};
use crate::state::AppState;

pub async fn store_audit_log(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    Query(filter): Query<AuditFilter>,
) -> Result<Json<Page<AuditEntry>>, ApiError> {
    let page = state.provider.audit_log(&tenant, store_id, filter).await?;
    Ok(Json(page))
}

pub async fn memory_audit_log(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId((store_id, memory_id)): PathId<(Uuid, Uuid)>,
) -> Result<Json<Page<AuditEntry>>, ApiError> {
    let filter = AuditFilter {
        memory_id: Some(memory_id),
        ..Default::default()
    };
    let page = state.provider.audit_log(&tenant, store_id, filter).await?;
    Ok(Json(page))
}

use axum::extract::State;
use axum::Json;
use uuid::Uuid;

use kd6_core::models::{SearchQuery, SearchResult};

use crate::error::ApiError;
use crate::extract::{JsonBody, PathId, TenantId};
use crate::state::AppState;

pub async fn search(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_id): PathId<Uuid>,
    JsonBody(query): JsonBody<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    let results = state.provider.search(&tenant, store_id, query).await?;
    Ok(Json(results))
}

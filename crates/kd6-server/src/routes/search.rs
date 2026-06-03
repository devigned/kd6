use axum::extract::State;
use axum::Json;

use kd6_core::models::{SearchQuery, SearchResult};

use crate::embed::auto_embed_query;
use crate::error::ApiError;
use crate::extract::{resolve_store, JsonBody, PathId, StoreRef, TenantId};
use crate::state::AppState;

pub async fn search(
    State(state): State<AppState>,
    TenantId(tenant): TenantId,
    PathId(store_ref): PathId<StoreRef>,
    JsonBody(mut query): JsonBody<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    let store_id = resolve_store(&store_ref, &tenant, &state).await?;

    // Auto-embed query if no embedding provided (OMS spec section 8.4.3)
    query.embedding = auto_embed_query(&*state.embedder, &query.query, query.embedding).await?;

    let results = state.provider.search(&tenant, store_id, query).await?;
    Ok(Json(results))
}

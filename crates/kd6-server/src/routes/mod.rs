pub mod audit;
pub mod batch;
pub mod gdpr;
pub mod graph;
pub mod health;
pub mod inheritance;
pub mod lifecycle;
pub mod memories;
pub mod search;
pub mod shared_spaces;
pub mod stores;

use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::state::AppState;

pub fn v1_routes() -> Router<AppState> {
    Router::new()
        // Store management
        .route("/stores", post(stores::create_store))
        .route("/stores", get(stores::list_stores))
        .route("/stores/{store_name}", get(stores::get_store))
        .route("/stores/{store_name}", patch(stores::update_store))
        .route("/stores/{store_name}", delete(stores::delete_store))
        // Memory CRUD
        .route(
            "/stores/{store_name}/memories",
            post(memories::create_memory),
        )
        .route(
            "/stores/{store_name}/memories",
            get(memories::list_memories),
        )
        .route(
            "/stores/{store_name}/memories/{memory_id}",
            get(memories::get_memory),
        )
        .route(
            "/stores/{store_name}/memories/{memory_id}",
            patch(memories::update_memory),
        )
        .route(
            "/stores/{store_name}/memories/{memory_id}",
            delete(memories::delete_memory),
        )
        // Search
        .route("/stores/{store_name}/search", post(search::search))
        // Audit
        .route("/stores/{store_name}/audit", get(audit::store_audit_log))
        .route(
            "/stores/{store_name}/memories/{memory_id}/audit",
            get(audit::memory_audit_log),
        )
        // Lifecycle
        .route(
            "/stores/{store_name}/expired",
            delete(lifecycle::purge_expired),
        )
        .route(
            "/stores/{store_name}/lifecycle/stats",
            get(lifecycle::lifecycle_stats),
        )
        // Batch
        .route(
            "/stores/{store_name}/memories/batch",
            post(batch::batch_create),
        )
        .route(
            "/stores/{store_name}/memories/batch/delete",
            post(batch::batch_delete),
        )
        // Inheritance
        .route(
            "/stores/{store_name}/inherit",
            post(inheritance::create_inheritance),
        )
        .route(
            "/stores/{store_name}/inherit/{inheritance_id}",
            delete(inheritance::delete_inheritance),
        )
        .route(
            "/stores/{store_name}/bubble-up",
            post(inheritance::bubble_up),
        )
        // Shared spaces
        .route(
            "/stores/{store_name}/shared-spaces",
            post(shared_spaces::create_shared_space),
        )
        .route(
            "/stores/{store_name}/shared-spaces",
            get(shared_spaces::list_shared_spaces),
        )
        .route(
            "/stores/{store_name}/shared-spaces/{space_id}",
            get(shared_spaces::get_shared_space),
        )
        .route(
            "/stores/{store_name}/shared-spaces/{space_id}/join",
            post(shared_spaces::join_shared_space),
        )
        .route(
            "/stores/{store_name}/shared-spaces/{space_id}/leave",
            post(shared_spaces::leave_shared_space),
        )
        .route(
            "/stores/{store_name}/shared-spaces/{space_id}",
            delete(shared_spaces::delete_shared_space),
        )
        // Graph (Level 3)
        .route("/stores/{store_name}/graph/edges", post(graph::create_edge))
        .route(
            "/stores/{store_name}/graph/edges/{edge_id}",
            delete(graph::delete_edge),
        )
        .route("/stores/{store_name}/graph/traverse", post(graph::traverse))
        // GDPR (Level 3)
        .route("/stores/{store_name}/gdpr/purge", post(gdpr::gdpr_purge))
}

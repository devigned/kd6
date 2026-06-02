pub mod error;
pub mod extract;
pub mod routes;
pub mod state;

use axum::Router;
use state::AppState;

/// Build the application router with all middleware.
/// Shared between main.rs and integration tests.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", axum::routing::get(routes::health::health))
        .route(
            "/capabilities",
            axum::routing::get(routes::health::capabilities),
        )
        .nest("/v1", routes::v1_routes())
        .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)) // 10 MB
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

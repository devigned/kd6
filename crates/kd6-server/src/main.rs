use std::sync::Arc;

use kd6_server::state::AppState;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let db_url =
        std::env::var("KD6_DATABASE_URL").unwrap_or_else(|_| "sqlite:kd6.db?mode=rwc".into());
    let provider = kd6_sqlite::SqliteProvider::new(&db_url).await?;

    let state = AppState {
        provider: Arc::new(provider),
    };

    let app = kd6_server::build_router(state);

    let addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    tracing::info!("listening on {addr}");
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

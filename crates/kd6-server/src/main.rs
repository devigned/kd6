use std::sync::Arc;

use kd6_core::NoopEmbedder;
use kd6_server::state::{AppState, ServerConfig};
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

    let auto_provision = std::env::var("KD6_AUTO_PROVISION")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);
    let default_tenant = std::env::var("KD6_DEFAULT_TENANT")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    // --- Embedding provider ---
    let embedding_provider =
        std::env::var("KD6_EMBEDDING_PROVIDER").unwrap_or_else(|_| "local".into());

    let embedder: Arc<dyn kd6_core::EmbeddingProvider> = match embedding_provider.as_str() {
        "local" => {
            tracing::info!("embedding provider: local (fastembed, in-process ONNX)");
            Arc::new(kd6_embed::LocalEmbedder::new()?)
        }
        "openai-compatible" => {
            let endpoint = std::env::var("KD6_EMBEDDING_ENDPOINT").expect(
                "KD6_EMBEDDING_ENDPOINT is required when KD6_EMBEDDING_PROVIDER=openai-compatible",
            );
            let model = std::env::var("KD6_EMBEDDING_MODEL").expect(
                "KD6_EMBEDDING_MODEL is required when KD6_EMBEDDING_PROVIDER=openai-compatible",
            );
            let api_key = std::env::var("KD6_EMBEDDING_API_KEY").ok();
            let dimensions: usize = std::env::var("KD6_EMBEDDING_DIMENSIONS")
                .unwrap_or_else(|_| "1536".into())
                .parse()
                .expect("KD6_EMBEDDING_DIMENSIONS must be a positive integer");
            tracing::info!(
                "embedding provider: openai-compatible (endpoint={endpoint}, model={model})"
            );
            Arc::new(
                kd6_embed::OpenAiCompatibleEmbedder::new(endpoint, model, api_key, dimensions)
                    .expect("failed to create OpenAI-compatible embedding provider"),
            )
        }
        "none" => {
            tracing::info!(
                "embedding provider: none (pass-through, callers must supply embeddings)"
            );
            Arc::new(NoopEmbedder)
        }
        other => {
            anyhow::bail!("unknown KD6_EMBEDDING_PROVIDER: {other}. Valid options: local, openai-compatible, none");
        }
    };

    tracing::info!(
        model_id = embedder.model_id(),
        dimensions = embedder.dimensions(),
        "embedding provider ready"
    );

    let state = AppState {
        provider: Arc::new(provider),
        embedder,
        config: ServerConfig {
            auto_provision,
            default_tenant,
        },
    };

    let app = kd6_server::build_router(state);

    let addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    tracing::info!("listening on {addr}");
    tracing::info!("auto_provision={auto_provision}, default_tenant={default_tenant}");
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

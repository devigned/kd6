use std::sync::Arc;

use kd6_core::{EmbeddingProvider, NoopEmbedder, OmsProvider};
use kd6_sqlite::SqliteProvider;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

use kd6_mcp::Kd6McpServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let db_url =
        std::env::var("KD6_DATABASE_URL").unwrap_or_else(|_| "sqlite:kd6.db?mode=rwc".to_string());

    let provider = Arc::new(SqliteProvider::new(&db_url).await?) as Arc<dyn OmsProvider>;

    // --- Embedding provider (same pattern as kd6-server) ---
    let embedding_provider =
        std::env::var("KD6_EMBEDDING_PROVIDER").unwrap_or_else(|_| "local".into());

    let embedder: Arc<dyn EmbeddingProvider> = match embedding_provider.as_str() {
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

    let transport = std::env::var("KD6_MCP_TRANSPORT").unwrap_or_else(|_| "http".to_string());

    match transport.as_str() {
        "stdio" => {
            tracing::info!("KD6 MCP server starting on stdio");
            let server = Kd6McpServer::new(provider, embedder);
            let service = server.serve(rmcp::transport::stdio()).await?;
            service.waiting().await?;
        }
        "http" => {
            let addr = std::env::var("KD6_MCP_ADDR").unwrap_or_else(|_| "0.0.0.0:8081".to_string());

            let service = StreamableHttpService::new(
                move || Ok(Kd6McpServer::new(provider.clone(), embedder.clone())),
                LocalSessionManager::default().into(),
                Default::default(),
            );

            let router = axum::Router::new().nest_service("/mcp", service);
            let listener = tokio::net::TcpListener::bind(&addr).await?;

            tracing::info!("KD6 MCP server listening on http://{addr}/mcp");

            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    tokio::signal::ctrl_c().await.unwrap();
                })
                .await?;
        }
        other => {
            anyhow::bail!("unknown KD6_MCP_TRANSPORT: {other}. Valid options: stdio, http");
        }
    }

    Ok(())
}

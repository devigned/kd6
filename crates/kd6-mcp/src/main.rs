use std::sync::Arc;

use kd6_core::OmsProvider;
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

    let db_url = std::env::var("KD6_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:kd6.db?mode=rwc".to_string());

    let provider = Arc::new(SqliteProvider::new(&db_url).await?) as Arc<dyn OmsProvider>;

    let transport = std::env::var("KD6_MCP_TRANSPORT")
        .unwrap_or_else(|_| "http".to_string());

    match transport.as_str() {
        "stdio" => {
            tracing::info!("KD6 MCP server starting on stdio");
            let server = Kd6McpServer::new(provider);
            let service = server.serve(rmcp::transport::stdio()).await?;
            service.waiting().await?;
        }
        _ => {
            let addr = std::env::var("KD6_MCP_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8081".to_string());

            let service = StreamableHttpService::new(
                move || Ok(Kd6McpServer::new(provider.clone())),
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
    }

    Ok(())
}

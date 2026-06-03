use std::sync::Arc;

use kd6_core::embedding::EmbeddingProvider;
use kd6_core::OmsProvider;

/// Configuration for optional spec features (OMS spec 4.1.1, 4.4.1).
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Enable `_default` store alias and auto-provisioning of stores on first write.
    pub auto_provision: bool,
    /// Enable default tenant resolution when `X-Tenant-ID` header is absent.
    pub default_tenant: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            auto_provision: true,
            default_tenant: true,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub provider: Arc<dyn OmsProvider>,
    pub embedder: Arc<dyn EmbeddingProvider>,
    pub config: ServerConfig,
}

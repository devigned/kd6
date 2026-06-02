use std::sync::Arc;

use kd6_core::OmsProvider;

#[derive(Clone)]
pub struct AppState {
    pub provider: Arc<dyn OmsProvider>,
}

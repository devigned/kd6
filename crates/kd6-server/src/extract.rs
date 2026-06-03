use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

/// The well-known default tenant identifier (OMS spec 4.4.1).
pub const DEFAULT_TENANT: &str = "_default";

/// The well-known default store alias (OMS spec 4.1.1).
pub const DEFAULT_STORE_ALIAS: &str = "_default";

/// Extracts tenant identity from `X-Tenant-ID` header.
/// Falls back to `_default` when the header is absent and default tenant
/// resolution is enabled in ServerConfig (OMS spec 4.4.1).
pub struct TenantId(pub String);

pub struct TenantIdRejection(&'static str);

impl IntoResponse for TenantIdRejection {
    fn into_response(self) -> Response {
        (StatusCode::UNAUTHORIZED, Json(json!({ "error": self.0 }))).into_response()
    }
}

impl FromRequestParts<AppState> for TenantId {
    type Rejection = TenantIdRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header_value = parts
            .headers
            .get("X-Tenant-ID")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        match header_value {
            Some(tenant) => Ok(TenantId(tenant)),
            None if state.config.default_tenant => Ok(TenantId(DEFAULT_TENANT.to_string())),
            None => Err(TenantIdRejection("X-Tenant-ID header is required")),
        }
    }
}

/// Resolved store identifier -- either a concrete UUID or the `_default` alias.
#[derive(Debug, Clone)]
pub enum StoreRef {
    Id(Uuid),
    Default,
}

impl<'de> serde::Deserialize<'de> for StoreRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == DEFAULT_STORE_ALIAS {
            Ok(StoreRef::Default)
        } else {
            Uuid::parse_str(&s)
                .map(StoreRef::Id)
                .map_err(serde::de::Error::custom)
        }
    }
}

/// Extracts agent identity from `X-Agent-ID` header (optional for now).
#[allow(dead_code)]
pub struct AgentId(pub String);

#[allow(dead_code)]
pub struct AgentIdRejection(&'static str);

impl IntoResponse for AgentIdRejection {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": self.0 }))).into_response()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for AgentId {
    type Rejection = AgentIdRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .headers
            .get("X-Agent-ID")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| AgentId(s.to_string()))
            .ok_or(AgentIdRejection("X-Agent-ID header is required"))
    }
}

/// JSON body extractor that returns `{"error": "..."}` on parse failure
/// instead of axum's default plain-text rejection.
pub struct JsonBody<T>(pub T);

pub struct JsonBodyRejection(JsonRejection);

impl IntoResponse for JsonBodyRejection {
    fn into_response(self) -> Response {
        let status = self.0.status();
        let message = self.0.body_text();
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl<S, T> axum::extract::FromRequest<S> for JsonBody<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = JsonBodyRejection;

    async fn from_request(
        req: axum::http::Request<axum::body::Body>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(JsonBodyRejection)?;
        Ok(JsonBody(value))
    }
}

/// Path extractor that returns `{"error": "..."}` on parse failure
/// instead of axum's default plain-text rejection.
pub struct PathId<T>(pub T);

pub struct PathIdRejection(PathRejection);

impl IntoResponse for PathIdRejection {
    fn into_response(self) -> Response {
        let status = self.0.status();
        let message = self.0.body_text();
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl<S, T> FromRequestParts<S> for PathId<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = PathIdRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(value) = axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map_err(PathIdRejection)?;
        Ok(PathId(value))
    }
}

/// Resolve a `StoreRef` to a concrete UUID. When the reference is `_default`
/// and auto-provisioning is enabled, the default store is created on demand.
pub async fn resolve_store(
    store_ref: &StoreRef,
    tenant_id: &str,
    state: &AppState,
) -> Result<Uuid, kd6_core::OmsError> {
    match store_ref {
        StoreRef::Id(id) => Ok(*id),
        StoreRef::Default => {
            if !state.config.auto_provision {
                return Err(kd6_core::OmsError::InvalidInput(
                    "_default store alias is disabled; create a store explicitly".into(),
                ));
            }
            let store = state
                .provider
                .get_or_create_store(
                    tenant_id,
                    DEFAULT_STORE_ALIAS,
                    kd6_core::models::CreateStoreRequest {
                        name: DEFAULT_STORE_ALIAS.to_string(),
                        region: None,
                        config: Default::default(),
                        metadata: Default::default(),
                    },
                )
                .await?;
            Ok(store.id)
        }
    }
}

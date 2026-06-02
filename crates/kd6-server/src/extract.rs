use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde_json::json;

/// Extracts tenant identity from `X-Tenant-ID` header.
pub struct TenantId(pub String);

pub struct TenantIdRejection(&'static str);

impl IntoResponse for TenantIdRejection {
    fn into_response(self) -> Response {
        (StatusCode::UNAUTHORIZED, Json(json!({ "error": self.0 }))).into_response()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for TenantId {
    type Rejection = TenantIdRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .headers
            .get("X-Tenant-ID")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| TenantId(s.to_string()))
            .ok_or(TenantIdRejection("X-Tenant-ID header is required"))
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

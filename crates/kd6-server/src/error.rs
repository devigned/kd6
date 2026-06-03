use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use kd6_core::OmsError;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self.0 {
            OmsError::StoreNotFound(ref id) => {
                (StatusCode::NOT_FOUND, format!("store not found: {id}"))
            }
            OmsError::MemoryNotFound(ref id) => {
                (StatusCode::NOT_FOUND, format!("memory not found: {id}"))
            }
            OmsError::InheritanceNotFound(ref id) => (
                StatusCode::NOT_FOUND,
                format!("inheritance not found: {id}"),
            ),
            OmsError::SpaceNotFound(ref id) => (
                StatusCode::NOT_FOUND,
                format!("shared space not found: {id}"),
            ),
            OmsError::NotFound(ref msg) => (StatusCode::NOT_FOUND, msg.clone()),
            OmsError::TenantRequired => (StatusCode::UNAUTHORIZED, "tenant required".into()),
            OmsError::Unauthorized(ref msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            OmsError::Forbidden(ref msg) => (StatusCode::FORBIDDEN, msg.clone()),
            OmsError::Conflict(ref msg) => (StatusCode::CONFLICT, msg.clone()),
            OmsError::ConstraintViolation(ref msg) => {
                (StatusCode::CONFLICT, format!("constraint violation: {msg}"))
            }
            OmsError::Immutable(ref id) => (
                StatusCode::CONFLICT,
                format!("immutable entry cannot be modified: {id}"),
            ),
            OmsError::InvalidInput(ref msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            OmsError::NotImplemented(ref msg) => (
                StatusCode::NOT_IMPLEMENTED,
                format!("not implemented: {msg}"),
            ),
            OmsError::Internal(ref msg) => {
                tracing::error!("internal error: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".into(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

/// Wrapper to convert OmsError into an axum response.
pub struct ApiError(pub OmsError);

impl From<OmsError> for ApiError {
    fn from(err: OmsError) -> Self {
        ApiError(err)
    }
}

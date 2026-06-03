use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OmsError {
    #[error("store not found: {0}")]
    StoreNotFound(String),

    #[error("memory not found: {0}")]
    MemoryNotFound(String),

    #[error("inheritance not found: {0}")]
    InheritanceNotFound(String),

    #[error("shared space not found: {0}")]
    SpaceNotFound(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("tenant required")]
    TenantRequired,

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("constraint violation: {0}")]
    ConstraintViolation(String),

    #[error("immutable entry cannot be modified: {0}")]
    Immutable(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("internal error: {0}")]
    Internal(String),
}

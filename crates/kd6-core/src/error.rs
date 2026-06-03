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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        assert_eq!(
            OmsError::StoreNotFound("s1".into()).to_string(),
            "store not found: s1"
        );
        assert_eq!(
            OmsError::MemoryNotFound("m1".into()).to_string(),
            "memory not found: m1"
        );
        assert_eq!(OmsError::TenantRequired.to_string(), "tenant required");
        assert_eq!(
            OmsError::ConstraintViolation("dup".into()).to_string(),
            "constraint violation: dup"
        );
        assert_eq!(
            OmsError::Immutable("locked".into()).to_string(),
            "immutable entry cannot be modified: locked"
        );
    }

    #[test]
    fn error_equality() {
        assert_eq!(OmsError::TenantRequired, OmsError::TenantRequired);
        assert_ne!(
            OmsError::StoreNotFound("a".into()),
            OmsError::StoreNotFound("b".into())
        );
        assert_ne!(
            OmsError::Internal("x".into()),
            OmsError::Conflict("x".into())
        );
    }
}

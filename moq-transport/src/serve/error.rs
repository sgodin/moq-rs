#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum ServeError {
    // TODO stop using?
    #[error("done")]
    Done,

    #[error("cancelled")]
    Cancel,

    #[error("closed, code={0}")]
    Closed(u64),

    #[error("not found")]
    NotFound,

    #[error("duplicate")]
    Duplicate,

    #[error("multiple stream modes")]
    Mode,

    #[error("wrong size")]
    Size,

    #[error("internal error: {0}")]
    Internal(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}

impl ServeError {
    /// Returns error codes for per-request/per-track errors.
    /// These codes are used in SUBSCRIBE_ERROR, PUBLISH_DONE, FETCH_ERROR, etc.
    /// Error codes are based on draft-ietf-moq-transport-14 Section 13.1.x
    pub fn code(&self) -> u64 {
        match self {
            // Special case: 0 typically means successful completion or internal error depending on context
            Self::Done => 0,
            // Cancel/Going away - maps to various contexts
            Self::Cancel => 1,
            // Pass through application-specific error codes
            Self::Closed(code) => *code,
            // TRACK_DOES_NOT_EXIST (0x4) from SUBSCRIBE_ERROR codes
            Self::NotFound => 0x4,
            // This is more of a session-level error, but keeping a reasonable code
            Self::Duplicate => 0x5,
            // NOT_SUPPORTED (0x3) - appears in multiple error code registries
            Self::Mode => 0x3,
            Self::Size => 0x3,
            Self::NotImplemented(_) => 0x3,
            // INTERNAL_ERROR (0x0) - per-request error registries use 0x0
            Self::Internal(_) => 0x0,
        }
    }
}

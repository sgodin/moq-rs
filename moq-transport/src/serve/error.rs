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

    #[error("not found [error:{0}]")]
    NotFoundWithId(uuid::Uuid),

    #[error("duplicate")]
    Duplicate,

    #[error("multiple stream modes")]
    Mode,

    #[error("wrong size")]
    Size,

    #[error("internal error: {0}")]
    Internal(String),

    #[error("internal error: {0} [error:{1}]")]
    InternalWithId(String, uuid::Uuid),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("not implemented: {0} [error:{1}]")]
    NotImplementedWithId(String, uuid::Uuid),
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
            Self::NotFound | Self::NotFoundWithId(_) => 0x4,
            // This is more of a session-level error, but keeping a reasonable code
            Self::Duplicate => 0x5,
            // NOT_SUPPORTED (0x3) - appears in multiple error code registries
            Self::Mode => 0x3,
            Self::Size => 0x3,
            Self::NotImplemented(_) | Self::NotImplementedWithId(_, _) => 0x3,
            // INTERNAL_ERROR (0x0) - per-request error registries use 0x0
            Self::Internal(_) | Self::InternalWithId(_, _) => 0x0,
        }
    }

    /// Create NotFound error with correlation ID and context logging.
    /// The correlation ID will be included in the error message sent to the client.
    #[track_caller]
    pub fn not_found_ctx(context: impl std::fmt::Display) -> Self {
        let id = uuid::Uuid::new_v4();
        let loc = std::panic::Location::caller();
        log::warn!(
            "[{}] Not found: {} at {}:{}",
            id,
            context,
            loc.file(),
            loc.line()
        );
        Self::NotFoundWithId(id)
    }

    /// Create Internal error with correlation ID and context logging.
    /// The correlation ID will be included in the error message sent to the client.
    #[track_caller]
    pub fn internal_ctx(context: impl std::fmt::Display) -> Self {
        let id = uuid::Uuid::new_v4();
        let loc = std::panic::Location::caller();
        log::error!(
            "[{}] Internal error: {} at {}:{}",
            id,
            context,
            loc.file(),
            loc.line()
        );
        Self::InternalWithId(context.to_string(), id)
    }

    /// Create NotImplemented error with correlation ID and context logging.
    /// The correlation ID will be included in the error message sent to the client.
    #[track_caller]
    pub fn not_implemented_ctx(feature: impl std::fmt::Display) -> Self {
        let id = uuid::Uuid::new_v4();
        let loc = std::panic::Location::caller();
        log::warn!(
            "[{}] Not implemented: {} at {}:{}",
            id,
            feature,
            loc.file(),
            loc.line()
        );
        Self::NotImplementedWithId(feature.to_string(), id)
    }
}

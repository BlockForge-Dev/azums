// src/jobs/error_codes.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
/// Canonical machine-readable failure code used by attempts and DLQ decisions.
/// # Examples
///
/// ```rust
/// use azums::jobs::error_codes::{suggested_action, ErrorCode};
///
/// let code = ErrorCode::from_str("TIMEOUT");
/// assert_eq!(code.as_str(), "TIMEOUT");
/// assert!(!suggested_action(code.as_str()).is_empty());
/// ```
pub enum ErrorCode {
    /// Handler execution exceeded its configured timeout.
    Timeout,
    /// Database transaction was selected as a deadlock victim.
    DbDeadlock,
    /// Serializable transaction or representation conflict occurred.
    Serialization,
    /// A producer or dependency rate limit was reached.
    RateLimit,
    /// Handler task panicked.
    Panic,
    /// Payload could not be validated or decoded.
    BadPayload,
    /// Failure is known not to benefit from retry.
    PermanentError,
    /// Required downstream dependency is unavailable.
    DependencyDown,
    /// Database connection was interrupted.
    DbDisconnect,
    /// Retryable execution-infrastructure failure occurred.
    SystemFailure,
    /// Job was explicitly cancelled.
    Cancelled,
    /// Worker stopped heartbeating and its lease expired.
    LeaseExpired,
    /// Handler returned an otherwise unclassified error.
    HandlerError,
    /// Error text did not match a known code.
    Unknown,
}

impl std::str::FromStr for ErrorCode {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_uppercase().as_str() {
            "TIMEOUT" => Self::Timeout,
            "DB_DEADLOCK" => Self::DbDeadlock,
            "SERIALIZATION" => Self::Serialization,
            "RATE_LIMIT" => Self::RateLimit,
            "PANIC" => Self::Panic,
            "BAD_PAYLOAD" => Self::BadPayload,
            "PERMANENT" | "PERMANENT_ERROR" => Self::PermanentError,
            "DEPENDENCY_DOWN" => Self::DependencyDown,
            "DB_DISCONNECT" => Self::DbDisconnect,
            "SYSTEM_FAILURE" => Self::SystemFailure,
            "CANCELLED" | "CANCELED" => Self::Cancelled,
            "LEASE_EXPIRED" => Self::LeaseExpired,
            "HANDLER_ERROR" => Self::HandlerError,
            _ => Self::Unknown,
        })
    }
}

/// # Examples
///
/// ```rust
/// use azums::jobs::error_codes::{suggested_action, ErrorCode};
///
/// let code = ErrorCode::from_str("TIMEOUT");
/// assert_eq!(code.as_str(), "TIMEOUT");
/// assert!(!suggested_action(code.as_str()).is_empty());
/// ```
impl ErrorCode {
    #[allow(clippy::should_implement_trait)]
    /// Parses an error code, returning [`ErrorCode::Unknown`] for unrecognized text.
    /// # Examples
    ///
    /// ```rust
    /// use azums::jobs::error_codes::{suggested_action, ErrorCode};
    ///
    /// let code = ErrorCode::from_str("TIMEOUT");
    /// assert_eq!(code.as_str(), "TIMEOUT");
    /// assert!(!suggested_action(code.as_str()).is_empty());
    /// ```
    pub fn from_str(s: &str) -> Self {
        s.parse().unwrap_or(Self::Unknown)
    }

    /// Returns the canonical uppercase representation stored in error records.
    /// # Examples
    ///
    /// ```rust
    /// use azums::jobs::error_codes::{suggested_action, ErrorCode};
    ///
    /// let code = ErrorCode::from_str("TIMEOUT");
    /// assert_eq!(code.as_str(), "TIMEOUT");
    /// assert!(!suggested_action(code.as_str()).is_empty());
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Timeout => "TIMEOUT",
            Self::DbDeadlock => "DB_DEADLOCK",
            Self::Serialization => "SERIALIZATION",
            Self::RateLimit => "RATE_LIMIT",
            Self::Panic => "PANIC",
            Self::BadPayload => "BAD_PAYLOAD",
            Self::PermanentError => "PERMANENT_ERROR",
            Self::DependencyDown => "DEPENDENCY_DOWN",
            Self::DbDisconnect => "DB_DISCONNECT",
            Self::SystemFailure => "SYSTEM_FAILURE",
            Self::Cancelled => "CANCELLED",
            Self::LeaseExpired => "LEASE_EXPIRED",
            Self::HandlerError => "HANDLER_ERROR",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// Returns concise operator guidance for a machine-readable failure code.
/// # Examples
///
/// ```rust
/// use azums::jobs::error_codes::{suggested_action, ErrorCode};
///
/// let code = ErrorCode::from_str("TIMEOUT");
/// assert_eq!(code.as_str(), "TIMEOUT");
/// assert!(!suggested_action(code.as_str()).is_empty());
/// ```
pub fn suggested_action(code: &str) -> &'static str {
    match ErrorCode::from_str(code) {
        ErrorCode::Timeout => {
            "Increase timeout OR reduce payload/work. Check downstream latency and retries."
        }
        ErrorCode::DbDeadlock => {
            "Retry is OK. Reduce lock contention: consistent row ordering, smaller transactions."
        }
        ErrorCode::Serialization => {
            "Retry is OK. Use SERIALIZABLE retry loop / reduce concurrent writes / use lower isolation if acceptable."
        }
        ErrorCode::RateLimit => {
            "Back off. Add client-side rate limiting, respect Retry-After, lower concurrency."
        }
        ErrorCode::Panic => {
            "Investigate crash. Capture panic info, add safeguards, consider marking permanent if deterministic."
        }
        ErrorCode::BadPayload | ErrorCode::PermanentError => {
            "Non-retryable. Validate payload schema/fields. Fix producer or add transform step."
        }
        ErrorCode::DependencyDown => {
            "Retry later. Check dependency health, circuit-break, alerting, fallback path."
        }
        ErrorCode::DbDisconnect | ErrorCode::SystemFailure | ErrorCode::LeaseExpired => {
            "Retry is OK. Check infrastructure health, worker lifecycle, and database/network stability."
        }
        ErrorCode::Cancelled => {
            "Cancellation is terminal. Inspect the caller or operator action that requested cancellation."
        }
        ErrorCode::HandlerError => {
            "Retryable by default. Add a specific error code if the failure is permanent or operational."
        }
        ErrorCode::Unknown => {
            "Inspect error_message + logs. Decide if retryable; add mapping once understood."
        }
    }
}

use rand::Rng;

#[derive(Debug, Clone)]
/// Exponential-backoff and jitter settings for retryable failures.
pub struct RetryConfig {
    /// Initial retry delay in seconds.
    pub base_seconds: i64,
    /// Maximum retry delay in seconds.
    pub max_seconds: i64,
    /// Symmetric jitter fraction applied to the calculated delay.
    pub jitter_pct: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            base_seconds: 2,
            max_seconds: 15 * 60,
            jitter_pct: 0.20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
/// Semantic failure class that determines retry and DLQ behavior.
pub enum ErrorClass {
    /// Handler failure that may succeed on a later attempt.
    Retryable,
    /// Deterministic failure that should enter DLQ immediately.
    Permanent,
    /// Handler exceeded its execution timeout and may retry.
    Timeout,
    /// Handler task panicked and enters DLQ.
    Panic,
    /// Execution was cancelled and is terminal.
    Cancelled,
    /// Retryable infrastructure or dependency failure.
    SystemFailure,
}

/// Maps a machine-readable error code to its semantic failure class.
pub fn classify_error(code: &str) -> ErrorClass {
    match code.trim().to_uppercase().as_str() {
        "TIMEOUT" => ErrorClass::Timeout,
        "PANIC" => ErrorClass::Panic,
        "CANCELLED" | "CANCELED" => ErrorClass::Cancelled,
        "BAD_PAYLOAD" | "UNKNOWN_JOB_TYPE" | "PERMANENT" | "PERMANENT_ERROR" => {
            ErrorClass::Permanent
        }
        "DEPENDENCY_DOWN" | "DB_DEADLOCK" | "SERIALIZATION" | "RATE_LIMIT" | "DB_DISCONNECT"
        | "SYSTEM_FAILURE" | "LEASE_EXPIRED" => ErrorClass::SystemFailure,
        _ => ErrorClass::Retryable,
    }
}

impl ErrorClass {
    /// Returns whether another attempt is permitted while budget remains.
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            ErrorClass::Retryable | ErrorClass::Timeout | ErrorClass::SystemFailure
        )
    }

    /// Returns the reason stored when this class enters the DLQ.
    pub fn dlq_reason_code(self) -> &'static str {
        match self {
            ErrorClass::Permanent => "PERMANENT_ERROR",
            ErrorClass::Panic => "PANIC",
            ErrorClass::Cancelled => "CANCELLED",
            ErrorClass::Timeout | ErrorClass::Retryable | ErrorClass::SystemFailure => {
                "MAX_ATTEMPTS_EXCEEDED"
            }
        }
    }
}

/// Parses an optional `CODE: message` handler error into canonical code and detail.
pub fn parse_handler_error(message: &str) -> (&'static str, &str) {
    let Some((code, rest)) = message.split_once(':') else {
        return ("HANDLER_ERROR", message);
    };

    let code = code.trim().to_uppercase();
    let known = matches!(
        code.as_str(),
        "TIMEOUT"
            | "BAD_PAYLOAD"
            | "PERMANENT"
            | "PERMANENT_ERROR"
            | "DEPENDENCY_DOWN"
            | "DB_DEADLOCK"
            | "SERIALIZATION"
            | "RATE_LIMIT"
            | "DB_DISCONNECT"
            | "SYSTEM_FAILURE"
            | "CANCELLED"
            | "CANCELED"
    );

    if known {
        let canonical = match code.as_str() {
            "PERMANENT" => "PERMANENT_ERROR",
            "CANCELED" => "CANCELLED",
            "TIMEOUT" => "TIMEOUT",
            "BAD_PAYLOAD" => "BAD_PAYLOAD",
            "PERMANENT_ERROR" => "PERMANENT_ERROR",
            "DEPENDENCY_DOWN" => "DEPENDENCY_DOWN",
            "DB_DEADLOCK" => "DB_DEADLOCK",
            "SERIALIZATION" => "SERIALIZATION",
            "RATE_LIMIT" => "RATE_LIMIT",
            "DB_DISCONNECT" => "DB_DISCONNECT",
            "SYSTEM_FAILURE" => "SYSTEM_FAILURE",
            "CANCELLED" => "CANCELLED",
            _ => "HANDLER_ERROR",
        };
        (canonical, rest.trim())
    } else {
        ("HANDLER_ERROR", message)
    }
}

/// Calculates capped exponential backoff with symmetric jitter for an attempt.
pub fn next_delay_seconds(attempt_no: i32, cfg: &RetryConfig, rng: &mut impl Rng) -> i64 {
    let attempt_no = attempt_no.max(1) as u32;

    // exponent = attempt_no - 1
    let exp = attempt_no.saturating_sub(1);

    // Compute 2^exp safely. If exp is too large, treat multiplier as huge and let cap handle it.
    let pow2 = 1_i64.checked_shl(exp).unwrap_or(i64::MAX);

    // base * 2^(attempt_no-1) with overflow protection
    let mut delay = cfg.base_seconds.saturating_mul(pow2);

    // cap
    if delay > cfg.max_seconds {
        delay = cfg.max_seconds;
    }

    // jitter in range [-jitter_pct, +jitter_pct]
    let jitter_range = (delay as f64) * cfg.jitter_pct;
    let jitter = rng.random_range(-jitter_range..=jitter_range);

    let jittered = (delay as f64 + jitter).round() as i64;
    jittered.clamp(0, cfg.max_seconds)
}

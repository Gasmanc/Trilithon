//! Error type for Caddy admin API operations.

/// Errors that can occur when communicating with the Caddy admin endpoint.
#[derive(Debug, thiserror::Error)]
pub enum CaddyError {
    /// The Caddy admin endpoint could not be reached.
    #[error("caddy admin endpoint unreachable: {detail}")]
    Unreachable {
        /// Human-readable detail about why the endpoint was unreachable.
        detail: String,
    },

    /// Caddy returned an unexpected HTTP status code.
    #[error("caddy responded {status}: {body}")]
    BadStatus {
        /// The HTTP status code returned.
        status: u16,
        /// The response body.
        body: String,
    },

    /// The ownership sentinel in the running config does not match the expected value.
    #[error("ownership sentinel mismatch (expected {expected}, found {found:?})")]
    OwnershipMismatch {
        /// The sentinel value this process expected to find.
        expected: String,
        /// The sentinel value actually found in the running config, if any.
        found: Option<String>,
    },

    /// The operation did not complete within the allowed time.
    #[error("operation timed out after {seconds}s")]
    Timeout {
        /// Timeout duration in seconds.
        seconds: u32,
    },

    /// Caddy returned a response that violates the expected admin protocol.
    #[error("caddy admin protocol violation: {detail}")]
    ProtocolViolation {
        /// Human-readable description of the violation.
        detail: String,
    },
}

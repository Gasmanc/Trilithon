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

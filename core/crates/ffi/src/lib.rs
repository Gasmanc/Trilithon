//! .-ffi — uniffi bridge between core/adapters and Swift.
//!
//! The Swift-facing API is declared in `core.udl`. Add functions/types here
//! that Swift needs to call. Keep this layer thin — it should only translate
//! types and call into core/adapters.

#![allow(clippy::missing_errors_doc)]

uniffi::include_scaffolding!("core");

/// Returns the version of the underlying core crate.
pub fn version() -> String {
    trilithon_core::version().to_string()
}

/// Boots adapters. Returns a human-readable error message on failure.
pub fn boot() -> Result<(), FfiError> {
    trilithon_adapters::boot().map_err(|e| FfiError::Boot(e.to_string()))
}

/// Errors that can occur in the FFI layer.
#[derive(Debug, Clone, thiserror::Error)]
pub enum FfiError {
    /// Error during boot initialization.
    #[error("boot failed: {0}")]
    Boot(String),
}

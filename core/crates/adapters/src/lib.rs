//! .-adapters — outside-world wrappers (db, http, fs, env).
//!
//! Depends on `core`. Translates between core types and external systems.

#![forbid(unsafe_code)]

pub use trilithon_core as core;

use anyhow::Result;

/// Initialize all adapters.
///
/// # Errors
///
/// This function does not currently return errors, but is designed to support
/// future initialization steps that may fail.
#[allow(clippy::unnecessary_wraps)]
pub fn boot() -> Result<()> {
    tracing::info!("adapters initialised");
    Ok(())
}

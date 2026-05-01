//! .-adapters — outside-world wrappers (db, http, fs, env).
//!
//! Depends on `core`. Translates between core types and external systems.

#![forbid(unsafe_code)]

pub mod config_loader;
pub mod env_provider;
pub mod lock;
pub mod sqlite_storage;

/// Errors returned by [`boot`].
#[derive(Debug, thiserror::Error)]
pub enum BootError {}

/// Initialize all adapters.
///
/// # Errors
///
/// This function does not currently return errors, but is designed to support
/// future initialization steps that may fail.
#[allow(clippy::unnecessary_wraps)]
// zd:phase-01 expires:2026-08-01 reason: boot() is a scaffold; will gain real fallible steps in Phase 2
pub fn boot() -> Result<(), BootError> {
    tracing::info!("adapters initialised");
    Ok(())
}

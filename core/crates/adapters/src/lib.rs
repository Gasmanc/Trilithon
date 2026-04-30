//! .-adapters — outside-world wrappers (db, http, fs, env).
//!
//! Depends on `core`. Translates between core types and external systems.

#![forbid(unsafe_code)]

pub use ._core as core;

use anyhow::Result;

/// Example adapter shape. Replace with real adapters.
pub fn boot() -> Result<()> {
    tracing::info!("adapters initialised");
    Ok(())
}

//! Trilithon-core — pure logic. No I/O. No async runtime.
//!
//! This crate must not depend on any I/O, network, filesystem, or process
//! crate. Adapters consume this crate and wire it to the outside world.

#![forbid(unsafe_code)]

use thiserror::Error;

pub mod caddy;
pub mod config;
pub mod exit;
pub mod lifecycle;
pub mod model;
pub mod mutation;
pub mod storage;

/// Errors from the core domain logic.
#[derive(Debug, Error)]
pub enum CoreError {
    /// Invalid input was provided.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// Returns the crate version.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

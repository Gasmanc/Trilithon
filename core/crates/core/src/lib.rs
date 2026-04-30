//! .-core — pure logic. No I/O. No async runtime.
//!
//! This crate must not depend on any I/O, network, filesystem, or process
//! crate. Adapters consume this crate and wire it to the outside world.

#![forbid(unsafe_code)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// Placeholder. Replace with your domain types.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

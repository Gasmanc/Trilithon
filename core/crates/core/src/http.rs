//! HTTP server abstraction — pure trait, no I/O.
//!
//! The default implementation lives in `crates/adapters/src/http_axum/`.

use std::net::SocketAddr;

use async_trait::async_trait;
use thiserror::Error;

pub use crate::config::types::ServerConfig;

/// A future-like shutdown signal passed to [`HttpServer::run`].
///
/// The implementor (typically in `cli`) calls the inner future when a signal
/// arrives. `adapters` consumes it via `Box<dyn ShutdownFuture>` so neither
/// side has a concrete dependency on the other.
pub type ShutdownSignal = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;

/// Errors produced by the HTTP server.
#[derive(Debug, Error)]
pub enum HttpServerError {
    /// The TCP bind failed.
    #[error("bind failed: {detail}")]
    BindFailed {
        /// Human-readable reason.
        detail: String,
    },

    /// The server crashed after binding.
    #[error("server crashed: {detail}")]
    Crashed {
        /// Human-readable reason.
        detail: String,
    },
}

/// The daemon's inbound HTTP face.
///
/// Bound to loopback by default; remote binding requires
/// `allow_remote = true` in `[server]` config (ADR-0011).
#[async_trait]
pub trait HttpServer: Send + 'static {
    /// Bind the configured listener.
    ///
    /// Returns the bound socket address so the caller can log and inject it
    /// into the audit log.
    async fn bind(&mut self, config: &ServerConfig) -> Result<SocketAddr, HttpServerError>;

    /// Run the server until graceful shutdown is requested.
    async fn run(self, shutdown: ShutdownSignal) -> Result<(), HttpServerError>;

    /// Trigger graceful shutdown from another task.
    async fn shutdown(&self) -> Result<(), HttpServerError>;
}

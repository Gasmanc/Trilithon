//! Lifecycle traits for graceful shutdown coordination across layers.
//!
//! [`ShutdownObserver`] is implemented on concrete shutdown handles (e.g. in
//! `cli`) so that `adapters` can consume it without importing from `cli`.

use async_trait::async_trait;

/// Implement this on your concrete shutdown handle so adapters can consume it.
///
/// The implementor should resolve `wait_for_shutdown` when a shutdown signal
/// has been received or the underlying channel has been closed.
#[async_trait]
pub trait ShutdownObserver: Send + 'static {
    /// Wait until a shutdown has been requested.
    ///
    /// Returns immediately if shutdown is already in progress.
    async fn wait_for_shutdown(&mut self);
}

//! Lifecycle traits for graceful shutdown coordination across layers.
//!
//! [`ShutdownObserver`] is implemented on concrete shutdown handles (e.g. in
//! `cli`) so that `adapters` can consume it without importing from `cli`.

/// Implement this on your concrete shutdown handle so adapters can consume it.
///
/// The implementor should resolve `changed` when a shutdown signal has been
/// received or the underlying channel has been closed.
pub trait ShutdownObserver: Send + 'static {
    /// Return a future that resolves when shutdown is signalled.
    ///
    /// The returned future borrows `self` mutably, so it must be polled to
    /// completion before `changed` can be called again.
    fn changed(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;

    /// Returns `true` if shutdown has already been signalled.
    fn is_shutting_down(&self) -> bool;
}

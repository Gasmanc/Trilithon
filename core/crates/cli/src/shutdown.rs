//! Shutdown plumbing: signal detection and graceful drain coordination.
//!
//! [`ShutdownController`] owns the send-side of a `watch` channel.
//! [`ShutdownSignal`] is the cloneable receive-side that tasks hold.
//!
//! Non-Unix targets are rejected at compile time; Trilithon V1 is Unix-only.

#[cfg(not(unix))]
compile_error!("Trilithon V1 supports Unix targets only (Linux, macOS). See ADR-0010.");

use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::watch;
use trilithon_core::lifecycle::ShutdownObserver;

/// Maximum wall-clock budget between SIGINT/SIGTERM receipt and process exit.
pub const DRAIN_BUDGET: Duration = Duration::from_secs(10);

/// The kind of OS signal that initiated shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// `SIGINT` (Ctrl-C / keyboard interrupt).
    Interrupt,
    /// `SIGTERM` (graceful-stop request from the OS or a process manager).
    Terminate,
}

/// Cloneable handle that tasks use to observe shutdown requests.
#[derive(Clone)]
pub struct ShutdownSignal {
    rx: watch::Receiver<bool>,
}

impl ShutdownSignal {
    /// Wait until shutdown has been requested.
    ///
    /// Returns immediately if shutdown is already in progress.  Loops on
    /// `rx.changed()` to guard against spurious wakes.
    pub async fn wait(&mut self) {
        // If already triggered, return immediately.
        if *self.rx.borrow() {
            return;
        }
        loop {
            // `changed()` resolves when the value changes from what we last saw.
            if self.rx.changed().await.is_err() {
                // Sender dropped — treat as shutdown.
                return;
            }
            if *self.rx.borrow() {
                return;
            }
        }
    }

    /// Returns `true` if a shutdown has already been triggered.
    ///
    /// Used by long-running tasks to poll shutdown state between checkpoints.
    #[expect(dead_code, reason = "spec-required API, callers added in later slices")]
    pub fn is_shutting_down(&self) -> bool {
        *self.rx.borrow()
    }
}

#[async_trait]
impl ShutdownObserver for ShutdownSignal {
    async fn wait_for_shutdown(&mut self) {
        self.wait().await;
    }
}

/// Owns the send-side of the shutdown channel.
pub struct ShutdownController {
    tx: watch::Sender<bool>,
}

impl ShutdownController {
    /// Create a new controller and its paired signal.
    pub fn new() -> (Self, ShutdownSignal) {
        let (tx, rx) = watch::channel(false);
        (Self { tx }, ShutdownSignal { rx })
    }

    /// Return a new [`ShutdownSignal`] cloned from the internal receiver.
    ///
    /// Use this to hand a signal to tasks spawned after the initial pair.
    #[expect(dead_code, reason = "spec-required API, callers added in later slices")]
    pub fn signal(&self) -> ShutdownSignal {
        ShutdownSignal {
            rx: self.tx.subscribe(),
        }
    }

    /// Broadcast the shutdown notification to all holders of [`ShutdownSignal`].
    pub fn trigger(&self) {
        let _ = self.tx.send(true);
    }
}

/// Block until `SIGINT` or `SIGTERM` arrives, then return which one it was.
///
/// Only available on Unix targets.
///
/// # Errors
///
/// Returns an error if the OS refuses to install signal handlers.
#[cfg(unix)]
pub async fn wait_for_signal() -> anyhow::Result<SignalKind> {
    use tokio::signal::unix::{SignalKind as TokioKind, signal};

    let mut sigint = signal(TokioKind::interrupt())
        .map_err(|e| anyhow::anyhow!("failed to install SIGINT handler: {e}"))?;
    let mut sigterm = signal(TokioKind::terminate())
        .map_err(|e| anyhow::anyhow!("failed to install SIGTERM handler: {e}"))?;

    Ok(tokio::select! {
        _ = sigint.recv() => SignalKind::Interrupt,
        _ = sigterm.recv() => SignalKind::Terminate,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ShutdownController, ShutdownSignal};

    /// `trigger()` must be observable by a task awaiting `signal.wait()`,
    /// and that task must complete within 100 ms.
    #[tokio::test]
    #[allow(clippy::expect_used, clippy::disallowed_methods)]
    async fn trigger_observable() {
        let (controller, mut signal): (ShutdownController, ShutdownSignal) =
            ShutdownController::new();

        let handle = tokio::spawn(async move {
            signal.wait().await;
        });

        controller.trigger();

        tokio::time::timeout(Duration::from_millis(100), handle)
            .await
            .expect("task did not complete within 100 ms")
            .expect("task panicked");
    }
}

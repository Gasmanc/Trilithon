//! Daemon run loop with signal handling and graceful shutdown.

use trilithon_core::exit::ExitCode;

use crate::shutdown::{
    DRAIN_BUDGET, ShutdownController, ShutdownSignal, SignalKind, wait_for_signal,
};

/// The placeholder daemon work task.
///
/// In Phase 1 this simply waits for the shutdown signal and returns.
async fn daemon_loop(mut signal: ShutdownSignal) {
    signal.wait().await;
}

/// Run the daemon until SIGINT or SIGTERM, then drain tasks within the budget.
///
/// # Errors
///
/// Returns an error if OS signal handler installation fails.
pub async fn run_with_shutdown() -> anyhow::Result<ExitCode> {
    let (controller, signal) = ShutdownController::new();

    let task = tokio::spawn(daemon_loop(signal));

    // Wait for a Unix signal.
    let kind = wait_for_signal().await?;
    match kind {
        SignalKind::Interrupt => {
            tracing::info!(reason = "sigint", "daemon.shutting-down");
        }
        SignalKind::Terminate => {
            tracing::info!(reason = "sigterm", "daemon.shutting-down");
        }
    }

    controller.trigger();

    // Await all spawned tasks, up to the drain budget.
    // A JoinError indicates the task panicked; treat that as an abnormal exit.
    match tokio::time::timeout(DRAIN_BUDGET, task).await {
        Ok(Ok(())) => {
            tracing::info!(forced = false, "daemon.shutdown-complete");
        }
        Ok(Err(join_err)) => {
            tracing::error!(error = %join_err, "daemon.task-panicked");
            return Ok(ExitCode::StartupPreconditionFailure);
        }
        Err(_elapsed) => {
            tracing::warn!(forced = true, "daemon.shutdown-complete");
        }
    }

    Ok(ExitCode::CleanShutdown)
}

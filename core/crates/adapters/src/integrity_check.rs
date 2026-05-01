//! Periodic `PRAGMA integrity_check` background task.

use std::time::Duration;

use sqlx::SqlitePool;
use tokio::time::{MissedTickBehavior, interval};
use trilithon_core::lifecycle::ShutdownObserver;

/// Default interval between integrity checks.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Result of a single `PRAGMA integrity_check` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityResult {
    /// `SQLite` reported `ok`.
    Ok,
    /// `SQLite` reported one or more problems.
    Failed {
        /// The raw detail string from `SQLite`; treat as opaque.
        detail: String,
    },
}

/// Run `PRAGMA integrity_check` once against `pool`.
///
/// # Errors
///
/// Returns `Err` if the query fails at the database driver level.
pub async fn integrity_check_once(pool: &SqlitePool) -> Result<IntegrityResult, sqlx::Error> {
    let row = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_one(pool)
        .await?;
    if row == "ok" {
        Ok(IntegrityResult::Ok)
    } else {
        Ok(IntegrityResult::Failed { detail: row })
    }
}

/// Spawn the periodic integrity-check loop.
///
/// The loop ticks every `every`, runs [`integrity_check_once`], and emits a
/// `tracing::error!` event on failure. It exits cleanly when `shutdown` fires.
pub async fn run_integrity_loop(
    pool: SqlitePool,
    every: Duration,
    mut shutdown: Box<dyn ShutdownObserver>,
) {
    let mut ticker = interval(every);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match integrity_check_once(&pool).await {
                    Ok(IntegrityResult::Ok) => {}
                    Ok(IntegrityResult::Failed { detail }) => {
                        tracing::error!(detail = %detail, "storage.integrity_check.failed");
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "storage.integrity_check.query_error");
                    }
                }
            }
            () = shutdown.wait_for_shutdown() => {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::disallowed_methods
    )]
    // reason: test-only code; panics are the correct failure mode in tests

    use super::{IntegrityResult, integrity_check_once};

    /// A healthy in-memory database should always report `Ok`.
    #[tokio::test]
    async fn healthy_db_reports_ok() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("failed to open in-memory db");

        let result = integrity_check_once(&pool).await.expect("query failed");
        assert_eq!(result, IntegrityResult::Ok);
    }
}

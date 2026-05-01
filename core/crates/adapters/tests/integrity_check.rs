//! Integration tests for the periodic integrity-check task.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::oneshot;
use trilithon_adapters::integrity_check::{
    IntegrityResult, integrity_check_once, run_integrity_loop,
};
use trilithon_core::lifecycle::ShutdownObserver;

/// A one-shot [`ShutdownObserver`] backed by a `tokio::sync::oneshot` channel.
struct OneShotShutdown(oneshot::Receiver<()>);

#[async_trait]
impl ShutdownObserver for OneShotShutdown {
    async fn wait_for_shutdown(&mut self) {
        let _ = (&mut self.0).await;
    }
}

/// A fresh in-memory database should always report `Ok`.
#[tokio::test]
async fn healthy_db_reports_ok() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to open in-memory db");

    let result = integrity_check_once(&pool).await.expect("query failed");
    assert_eq!(result, IntegrityResult::Ok);
}

/// Firing the shutdown signal should cause `run_integrity_loop` to resolve.
#[tokio::test]
async fn shutdown_breaks_the_loop() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to open in-memory db");

    let (tx, rx) = oneshot::channel::<()>();
    let shutdown = Box::new(OneShotShutdown(rx));

    let handle = tokio::spawn(run_integrity_loop(
        pool,
        Duration::from_millis(100),
        shutdown,
    ));

    // Fire shutdown after 50 ms.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = tx.send(());

    tokio::time::timeout(Duration::from_millis(500), handle)
        .await
        .expect("run_integrity_loop did not terminate within 500 ms")
        .expect("task panicked");
}

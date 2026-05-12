//! Integration tests for the migration runner.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::str::FromStr;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use trilithon_adapters::migrate::{MigrationError, apply_migrations};

async fn test_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite://:memory:")
        .unwrap()
        .journal_mode(SqliteJournalMode::Wal)
        .create_if_missing(true);
    SqlitePoolOptions::new().connect_with(opts).await.unwrap()
}

/// A fresh database should have all migrations applied.
#[tokio::test]
async fn fresh_db_applies_all() {
    let pool = test_pool().await;
    let outcome = apply_migrations(&pool)
        .await
        .expect("migrations should succeed on a fresh DB");

    assert!(
        outcome.applied_count >= 1,
        "expected at least one migration to be applied, got {}",
        outcome.applied_count
    );
    assert_eq!(
        outcome.current_version, 9,
        "expected current_version == 9 after initial migrations (including 0009_drift_events)"
    );
}

/// Running migrations twice on the same DB should be idempotent.
#[tokio::test]
async fn idempotent_second_run() {
    let pool = test_pool().await;

    // First run — applies everything.
    apply_migrations(&pool)
        .await
        .expect("first migration run should succeed");

    // Second run — nothing new to apply.
    let outcome = apply_migrations(&pool)
        .await
        .expect("second migration run should succeed");

    assert_eq!(
        outcome.applied_count, 0,
        "second run should report zero applied migrations"
    );
}

/// If the database reports a version higher than the embedded set, the runner
/// must refuse to start.
#[tokio::test]
async fn refuses_downgrade() {
    let pool = test_pool().await;

    // First run — get the DB into a known-good state.
    apply_migrations(&pool)
        .await
        .expect("initial migration run should succeed");

    // Simulate a future migration by inserting a row with version=999 into
    // sqlx's internal tracking table.
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, installed_on, checksum, execution_time, success) \
         VALUES (999, 'future', 0, X'', 0, 1)",
    )
    .execute(&pool)
    .await
    .expect("should be able to insert fake future migration row");

    // Now the DB claims version 999 — the runner must refuse.
    let err = apply_migrations(&pool)
        .await
        .expect_err("should refuse to start when DB version exceeds embedded max");

    match err {
        MigrationError::Downgrade {
            db_version,
            embedded_max,
        } => {
            assert_eq!(db_version, 999, "reported db_version should be 999");
            assert_eq!(embedded_max, 9, "reported embedded_max should be 9");
        }
        MigrationError::Sqlx { source } => {
            panic!("expected Downgrade error, got Sqlx: {source}");
        }
        MigrationError::Read { source } => {
            panic!("expected Downgrade error, got Read: {source}");
        }
    }
}

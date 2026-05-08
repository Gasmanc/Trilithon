//! Integration test: the migration set (including `0006_audit_immutable.sql`)
//! is idempotent under repeated invocation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use trilithon_adapters::migrate::apply_migrations;

async fn test_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite://:memory:")
        .unwrap()
        .journal_mode(SqliteJournalMode::Wal)
        .create_if_missing(true);
    SqlitePoolOptions::new().connect_with(opts).await.unwrap()
}

/// Running migrations twice on the same database is a no-op on the second run.
#[tokio::test]
async fn migrations_idempotent_second_run() {
    let pool = test_pool().await;

    // First run — applies all migrations including 0006_audit_immutable.
    let first = apply_migrations(&pool)
        .await
        .expect("first migration run must succeed");

    assert!(
        first.applied_count >= 1,
        "at least one migration must be applied on first run"
    );
    assert!(
        first.current_version >= 6,
        "current_version must be at least 6 after 0006_audit_immutable is applied; got {}",
        first.current_version
    );

    // Second run — nothing new to apply.
    let second = apply_migrations(&pool)
        .await
        .expect("second migration run must succeed");

    assert_eq!(
        second.applied_count, 0,
        "second run must report zero applied migrations"
    );
    assert_eq!(
        second.current_version, first.current_version,
        "current_version must not change on a no-op run"
    );
}

/// After migration, the `audit_log_no_update` and `audit_log_no_delete`
/// triggers must exist in `sqlite_master`.
#[tokio::test]
async fn audit_immutability_triggers_exist_after_migration() {
    let pool = test_pool().await;
    apply_migrations(&pool)
        .await
        .expect("migrations must succeed");

    let update_trigger: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND name='audit_log_no_update'",
    )
    .fetch_optional(&pool)
    .await
    .expect("query for trigger must succeed");

    assert!(
        update_trigger.is_some(),
        "audit_log_no_update trigger must exist after migration"
    );

    let delete_trigger: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND name='audit_log_no_delete'",
    )
    .fetch_optional(&pool)
    .await
    .expect("query for trigger must succeed");

    assert!(
        delete_trigger.is_some(),
        "audit_log_no_delete trigger must exist after migration"
    );
}

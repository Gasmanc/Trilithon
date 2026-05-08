//! Integration test: `UPDATE audit_log` is aborted by the immutability trigger.

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
use trilithon_core::storage::helpers::audit_prev_hash_seed;

async fn test_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite://:memory:")
        .unwrap()
        .journal_mode(SqliteJournalMode::Wal)
        .create_if_missing(true);
    SqlitePoolOptions::new().connect_with(opts).await.unwrap()
}

/// Insert a minimal `audit_log` row directly via SQL (bypassing the Storage
/// trait) so we can exercise the trigger without needing the full stack.
async fn insert_raw_row(pool: &SqlitePool, id: &str) {
    sqlx::query(
        r"
        INSERT INTO audit_log
            (id, prev_hash, caddy_instance_id, correlation_id, occurred_at, occurred_at_ms,
             actor_kind, actor_id, kind, redaction_sites, outcome)
        VALUES (?, ?, 'local', 'corr-01', 1700000000, 1700000000000,
                'system', 'test', 'config.applied', 0, 'ok')
        ",
    )
    .bind(id)
    .bind(audit_prev_hash_seed())
    .execute(pool)
    .await
    .expect("raw INSERT into audit_log must succeed");
}

/// Attempting `UPDATE audit_log SET notes = 'changed' WHERE id = ?` must be
/// aborted by the `audit_log_no_update` trigger and the error must propagate.
#[tokio::test]
async fn update_audit_log_aborts() {
    let pool = test_pool().await;
    apply_migrations(&pool)
        .await
        .expect("migrations must succeed");

    let id = "01JTEST000000000000000UPDATE";
    insert_raw_row(&pool, id).await;

    let result = sqlx::query("UPDATE audit_log SET notes = 'changed' WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await;

    assert!(
        result.is_err(),
        "UPDATE audit_log must fail; immutability trigger should abort"
    );

    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("immutable"),
        "error message must mention 'immutable'; got: {err_str}"
    );
}

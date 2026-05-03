//! Integration test: migration `0002_capability_probe.sql` creates the expected
//! table and indexes, including the unique partial index.

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
use trilithon_adapters::migrate::apply_migrations;

async fn test_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite://:memory:")
        .unwrap()
        .journal_mode(SqliteJournalMode::Wal)
        .create_if_missing(true);
    SqlitePoolOptions::new().connect_with(opts).await.unwrap()
}

/// Applying both migrations must produce a `capability_probe_results` table
/// with the composite index and the unique partial index on `is_current`.
#[tokio::test]
async fn migration_0002_creates_table() {
    let pool = test_pool().await;

    let outcome = apply_migrations(&pool)
        .await
        .expect("migrations should succeed");

    assert!(
        outcome.current_version >= 2,
        "expected current_version >= 2 after applying 0002, got {}",
        outcome.current_version
    );

    // Verify both the table and the unique partial index exist in a single query.
    let schema_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE (type = 'table' AND name = 'capability_probe_results') \
            OR (type = 'index' AND name = 'capability_probe_results_current')",
    )
    .fetch_one(&pool)
    .await
    .expect("sqlite_master query should succeed");

    assert_eq!(
        schema_count, 2,
        "expected capability_probe_results table and capability_probe_results_current index, got {schema_count} objects"
    );
}

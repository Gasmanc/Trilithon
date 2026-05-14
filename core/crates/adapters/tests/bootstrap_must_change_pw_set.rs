//! Bootstrap sets `must_change_pw = true` on the created admin user.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::sync::Arc;

use tempfile::TempDir;
use trilithon_adapters::{
    AuditWriter, Sha256AuditHasher,
    auth::{bootstrap::bootstrap_if_empty, users::SqliteUserStore},
    migrate::apply_migrations,
    rng::ThreadRng,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{clock::SystemClock, schema::SchemaRegistry, storage::trait_def::Storage};

#[tokio::test]
async fn bootstrap_must_change_pw_set() {
    let dir = TempDir::new().unwrap();
    let storage = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(storage.pool()).await.expect("migrations");
    let pool = storage.pool().clone();
    let user_store = SqliteUserStore::new(pool);

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    let audit = AuditWriter::new_with_arcs(
        storage_arc,
        Arc::new(SystemClock),
        Arc::new(SchemaRegistry::with_tier1_secrets()),
        Arc::new(Sha256AuditHasher),
    );

    let outcome = bootstrap_if_empty(&user_store, &ThreadRng, dir.path(), &audit)
        .await
        .expect("bootstrap_if_empty must succeed")
        .expect("must return Some on a fresh store");

    assert!(
        outcome.user.must_change_pw,
        "bootstrap user must have must_change_pw = true"
    );
}

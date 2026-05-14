//! Bootstrap returns `Ok(None)` when at least one user already exists.

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
    auth::{
        UserRole,
        bootstrap::bootstrap_if_empty,
        users::{SqliteUserStore, UserStore as _},
    },
    migrate::apply_migrations,
    rng::ThreadRng,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{clock::SystemClock, schema::SchemaRegistry, storage::trait_def::Storage};

#[tokio::test]
async fn bootstrap_skips_when_users_exist() {
    let dir = TempDir::new().unwrap();
    let storage = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(storage.pool()).await.expect("migrations");
    let pool = storage.pool().clone();
    let user_store = SqliteUserStore::new(pool);

    // Pre-populate one user.
    user_store
        .create_user("existing", "s3cr3t!", UserRole::Reader)
        .await
        .expect("create_user must succeed");

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    let audit = AuditWriter::new_with_arcs(
        storage_arc,
        Arc::new(SystemClock),
        Arc::new(SchemaRegistry::with_tier1_secrets()),
        Arc::new(Sha256AuditHasher),
    );

    let result = bootstrap_if_empty(&user_store, &ThreadRng, dir.path(), &audit)
        .await
        .expect("bootstrap_if_empty must not error");

    assert!(
        result.is_none(),
        "bootstrap must return None when users already exist"
    );
}

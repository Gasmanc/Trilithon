//! Bootstrap creates `bootstrap-credentials.txt` with mode 0600 on Unix.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::os::unix::fs::PermissionsExt as _;
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

async fn make_audit_and_store(dir: &TempDir) -> (AuditWriter, SqliteUserStore) {
    let storage = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(storage.pool()).await.expect("migrations");
    let pool = storage.pool().clone();
    let user_store = SqliteUserStore::new(pool);

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    let clock = Arc::new(SystemClock);
    let registry = Arc::new(SchemaRegistry::with_tier1_secrets());
    let hasher = Arc::new(Sha256AuditHasher);
    let audit = AuditWriter::new_with_arcs(storage_arc, clock, registry, hasher);
    (audit, user_store)
}

#[tokio::test]
async fn bootstrap_creates_file_mode_0600() {
    let dir = TempDir::new().unwrap();
    let (audit, user_store) = make_audit_and_store(&dir).await;

    let outcome = bootstrap_if_empty(&user_store, &ThreadRng, dir.path(), &audit)
        .await
        .expect("bootstrap_if_empty must succeed")
        .expect("must return Some on a fresh store");

    let meta = std::fs::metadata(&outcome.credentials_path).expect("credentials file must exist");
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "credentials file must have mode 0600, got {mode:#o}"
    );
}

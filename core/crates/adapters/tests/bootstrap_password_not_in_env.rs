//! Bootstrap does not store the plaintext password in any environment variable.
//!
//! This is a spec-required test (Slice 9.4): the generated password must not
//! appear as the value of any environment variable visible to the process.

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
    rng::RandomBytes,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{clock::SystemClock, schema::SchemaRegistry};

struct FixedRng;

impl RandomBytes for FixedRng {
    fn fill_bytes(&self, buf: &mut [u8]) {
        buf.fill(0xAB);
    }
}

#[tokio::test]
async fn bootstrap_password_not_in_env() {
    let dir = TempDir::new().unwrap();
    let storage = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(storage.pool()).await.expect("migrations");

    let pool = storage.pool().clone();
    let storage_arc = Arc::new(storage);
    let schema_registry = Arc::new(SchemaRegistry::default());
    let hasher = Arc::new(Sha256AuditHasher);
    let clock = Arc::new(SystemClock);
    let audit = AuditWriter::new_with_arcs(storage_arc, clock, schema_registry, hasher);
    let user_store = SqliteUserStore::new(pool);
    let rng = FixedRng;

    let outcome = bootstrap_if_empty(&user_store, &rng, dir.path(), &audit)
        .await
        .unwrap()
        .expect("bootstrap must create an account on an empty store");

    let credentials_path = &outcome.credentials_path;
    let credentials_content = std::fs::read_to_string(credentials_path).unwrap();
    // Extract the password line ("password: <value>").
    let password = credentials_content
        .lines()
        .find_map(|l| l.strip_prefix("password: "))
        .expect("credentials file must contain a 'password: ' line");

    // Assert the password does not appear in any environment variable value.
    for (key, value) in std::env::vars() {
        assert!(
            !value.contains(password),
            "env var {key} contains the bootstrap password — this is a security violation"
        );
    }
}

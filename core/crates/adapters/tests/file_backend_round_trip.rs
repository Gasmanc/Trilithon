//! Write then read; assert byte equality.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::panic
)]
// reason: integration test — panics are the correct failure mode

use tempfile::TempDir;
use trilithon_adapters::secrets_local::{FileBackend, MasterKeyBackend as _};

#[tokio::test]
async fn round_trip_returns_same_key() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("master-key");
    let backend = FileBackend { path: path.clone() };

    let first = backend.load_or_generate().await.unwrap();
    let second = backend.load_or_generate().await.unwrap();

    assert_eq!(first, second, "second load should return the same key");
}

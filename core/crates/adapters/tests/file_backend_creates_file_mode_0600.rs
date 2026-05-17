//! Unix-only: first call creates the key file with mode 0600.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::panic
)]
// reason: integration test — panics are the correct failure mode

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::PermissionsExt as _;

    use tempfile::TempDir;
    use trilithon_adapters::secrets_local::{FileBackend, MasterKeyBackend as _};

    #[tokio::test]
    async fn creates_file_mode_0600() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("master-key");
        let backend = FileBackend { path: path.clone() };
        backend.load_or_generate().await.unwrap();

        let meta = std::fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected mode 0600, got {mode:04o}");
    }
}

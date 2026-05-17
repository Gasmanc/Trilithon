//! Pre-create the file as 0o644; assert it is reset to 0o600.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::panic
)]
// reason: integration test — panics are the correct failure mode

#[cfg(unix)]
mod unix {
    use std::io::Write as _;
    use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

    use tempfile::TempDir;
    use trilithon_adapters::secrets_local::{FileBackend, MasterKeyBackend as _};

    #[tokio::test]
    async fn tightens_permissions_from_0644() {
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("master-key");

        // Write a valid key file with overly-permissive mode.
        let key = [0x42u8; 32];
        let b64 = BASE64.encode(key);
        let content = format!("version=1\nkey={b64}\n");
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o644)
            .open(&path)
            .unwrap();
        file.write_all(content.as_bytes()).unwrap();
        drop(file);

        let backend = FileBackend { path: path.clone() };
        let loaded = backend.load_or_generate().await.unwrap();
        assert_eq!(loaded, key, "loaded key should match written key");

        let meta = std::fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "mode should have been tightened to 0600, got {mode:04o}"
        );
    }
}

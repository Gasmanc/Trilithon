//! Assert the startup warning fires when `FileBackend::load_or_generate` is called.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

use tempfile::TempDir;
use trilithon_adapters::secrets_local::{FileBackend, MasterKeyBackend as _};

fn raw_logs_contain(needle: &str) -> bool {
    let logs = {
        let buf = tracing_test::internal::global_buf().lock().expect("lock");
        String::from_utf8(buf.clone()).expect("utf8")
    };
    logs.contains(needle)
}

#[tracing_test::traced_test]
#[tokio::test]
async fn startup_warning_emitted_on_load_or_generate() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("master-key");
    let backend = FileBackend { path };
    backend.load_or_generate().await.unwrap();

    assert!(
        raw_logs_contain("back up") && raw_logs_contain("out-of-band"),
        "startup warning must mention out-of-band backup"
    );
}

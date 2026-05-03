//! End-to-end integration test: ownership sentinel against a real Caddy binary.
//!
//! Gated on environment variable `TRILITHON_E2E_CADDY=1`.
//!
//! The test starts Caddy with a hand-crafted config that already contains a
//! foreign ownership sentinel.  It then runs [`ensure_sentinel`] without
//! `--takeover` and verifies that [`SentinelError::Conflict`] is returned.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: integration test — panics and unwrap are correct failure modes here

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use tempfile::TempDir;
use trilithon_adapters::caddy::{
    hyper_client::HyperCaddyClient,
    sentinel::{SentinelError, ensure_sentinel},
};
use trilithon_core::config::CaddyEndpoint;

struct CaddyProcess {
    child: Child,
    _tmp: TempDir,
}

impl Drop for CaddyProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Caddy JSON config that includes a foreign ownership sentinel.
fn foreign_sentinel_config(socket_path: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "admin": {
            "listen": format!("unix/{}", socket_path.display()),
            "enforce_origin": false
        },
        "apps": {
            "http": {
                "servers": {
                    "__trilithon_sentinel__": {
                        "@id": "trilithon-owner",
                        "installation_id": "foreign-installation-id"
                    }
                }
            }
        }
    })
}

/// Launch Caddy with a config that has a foreign sentinel embedded.
fn launch_caddy_with_foreign_sentinel() -> (CaddyProcess, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let socket_path = tmp.path().join("caddy-admin.sock");
    let config_path = tmp.path().join("caddy.json");

    let config = foreign_sentinel_config(&socket_path);
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("json"),
    )
    .expect("write config");

    let child = Command::new("caddy")
        .args(["run", "--config", config_path.to_str().expect("path")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn caddy; ensure caddy 2.8+ is in PATH");

    let proc = CaddyProcess { child, _tmp: tmp };
    (proc, socket_path)
}

/// Wait until the Unix socket exists (up to `timeout`).
fn wait_for_socket(path: &std::path::Path, timeout: Duration) {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!(
        "Caddy socket {} never appeared within {timeout:?}",
        path.display()
    );
}

/// A foreign sentinel in Caddy's running config, without `--takeover`, must
/// return a [`SentinelError::Conflict`] containing the conflicting identifier.
///
/// In a real binary invocation this maps to exit code 3.
#[tokio::test]
async fn foreign_sentinel_exits_3() {
    if std::env::var("TRILITHON_E2E_CADDY").as_deref() != Ok("1") {
        // Skip unless explicitly enabled.
        return;
    }

    let (_caddy, socket_path) = launch_caddy_with_foreign_sentinel();
    wait_for_socket(&socket_path, Duration::from_secs(10));

    let endpoint = CaddyEndpoint::Unix {
        path: socket_path.clone(),
    };
    let client =
        HyperCaddyClient::from_config(&endpoint, Duration::from_secs(5), Duration::from_secs(10))
            .expect("build client");

    let result = ensure_sentinel(&client, "our-installation-id", false).await;

    let err = result.expect_err("expected Conflict error");
    assert!(
        matches!(
            &err,
            SentinelError::Conflict { found, ours }
                if found == "foreign-installation-id" && ours == "our-installation-id"
        ),
        "unexpected error: {err}",
    );

    // In the binary, SentinelError::Conflict maps to exit code 3
    // (ExitCode::StartupPreconditionFailure).  The error message must contain
    // the conflicting identifier so operators can identify the other instance.
    let msg = err.to_string();
    assert!(
        msg.contains("foreign-installation-id"),
        "error message must contain the conflicting id; got: {msg}",
    );
}

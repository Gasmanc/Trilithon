//! End-to-end integration test: real Caddy 2.8 binary on a temp Unix socket.
//!
//! Gated on environment variable `TRILITHON_E2E_CADDY=1`.
//! The test launches Caddy on a temporary Unix socket, posts a minimal JSON
//! config via `load_config`, retrieves it with `get_running_config`, and
//! asserts equality.

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
use trilithon_adapters::caddy::hyper_client::HyperCaddyClient;
use trilithon_core::{
    caddy::{client::CaddyClient, types::CaddyConfig},
    config::CaddyEndpoint,
};

/// Minimal Caddy JSON config.  The `admin` block explicitly sets the socket.
fn minimal_caddy_config(socket_path: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "admin": {
            "listen": format!("unix/{}", socket_path.display()),
            "enforce_origin": false
        }
    })
}

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

/// Launch `caddy run` with an inline JSON config on a temporary Unix socket.
///
/// Returns the process handle and the socket path.
fn launch_caddy() -> (CaddyProcess, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let socket_path = tmp.path().join("caddy-admin.sock");
    let config_path = tmp.path().join("caddy.json");

    let config = minimal_caddy_config(&socket_path);
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

#[tokio::test]
async fn round_trip_load_then_get() {
    if std::env::var("TRILITHON_E2E_CADDY").as_deref() != Ok("1") {
        // Skip unless explicitly enabled.
        return;
    }

    let (_caddy, socket_path) = launch_caddy();
    wait_for_socket(&socket_path, Duration::from_secs(10));

    let endpoint = CaddyEndpoint::Unix {
        path: socket_path.clone(),
    };
    let client =
        HyperCaddyClient::from_config(&endpoint, Duration::from_secs(5), Duration::from_secs(10))
            .expect("build client");

    // Health check
    let health = client.health_check().await.expect("health_check");
    assert_eq!(
        health,
        trilithon_core::caddy::types::HealthState::Reachable,
        "Caddy should be reachable"
    );

    // Load a minimal config
    let config_value = serde_json::json!({
        "admin": {
            "listen": format!("unix/{}", socket_path.display()),
            "enforce_origin": false
        }
    });
    let config = CaddyConfig(config_value.clone());
    client.load_config(config).await.expect("load_config");

    // GET the running config
    let running = client
        .get_running_config()
        .await
        .expect("get_running_config");

    // The admin block should survive the round trip.
    assert_eq!(
        running.0.get("admin"),
        config_value.get("admin"),
        "admin block should round-trip"
    );
}

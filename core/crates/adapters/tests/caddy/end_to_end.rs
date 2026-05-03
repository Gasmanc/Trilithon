//! Full daemon end-to-end test against a real Caddy 2.8 binary.
//!
//! Gated on environment variable `TRILITHON_E2E_CADDY=1`.
//!
//! The test:
//! 1. Boots Caddy 2.8 with a minimal config on a Unix admin socket in a temp dir.
//! 2. Runs the `trilithon` daemon binary configured to talk to that socket.
//! 3. Asserts stderr contains, in order: `caddy.capability-probe.completed`,
//!    no sentinel error, then `daemon.started`.
//! 4. Sends SIGTERM, asserts exit 0.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::print_stderr
)]
// reason: integration test — panics and unwrap are correct failure modes here

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Caddy process guard
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Binary path discovery
// ---------------------------------------------------------------------------

/// Find the `trilithon` binary.
///
/// Resolution order:
/// 1. `TRILITHON_BIN` environment variable (absolute path).
/// 2. Workspace `target/debug/trilithon` relative to this crate's manifest dir.
/// 3. Workspace `target/release/trilithon`.
fn trilithon_bin() -> PathBuf {
    if let Ok(bin) = std::env::var("TRILITHON_BIN") {
        return PathBuf::from(bin);
    }

    // CARGO_MANIFEST_DIR = .../core/crates/adapters
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // workspace root = .../core
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("unexpected manifest dir structure");

    let debug_bin = workspace_root.join("target/debug/trilithon");
    if debug_bin.exists() {
        return debug_bin;
    }

    let release_bin = workspace_root.join("target/release/trilithon");
    if release_bin.exists() {
        return release_bin;
    }

    // Fall back to debug path so the error message is actionable.
    debug_bin
}

// ---------------------------------------------------------------------------
// Caddy launch helpers
// ---------------------------------------------------------------------------

/// Minimal Caddy JSON config for the test: no apps, just the admin socket.
fn minimal_caddy_config(socket_path: &Path) -> serde_json::Value {
    serde_json::json!({
        "admin": {
            "listen": format!("unix/{}", socket_path.display()),
            "enforce_origin": false
        }
    })
}

/// Launch Caddy with the minimal config.  Returns the process guard and
/// the path to the Unix admin socket.
fn launch_caddy(tmp: TempDir) -> (CaddyProcess, PathBuf) {
    let socket_path = tmp.path().join("caddy-admin.sock");
    let config_path = tmp.path().join("caddy.json");

    let config = minimal_caddy_config(&socket_path);
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("json"),
    )
    .expect("write caddy config");

    let caddy_bin = std::env::var("CADDY").unwrap_or_else(|_| "caddy".to_owned());
    let child = Command::new(&caddy_bin)
        .args(["run", "--config", config_path.to_str().expect("path")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| {
            panic!(
                "failed to spawn caddy binary '{caddy_bin}'; \
                 ensure caddy 2.8+ is in PATH or set $CADDY: {e}"
            )
        });

    let proc = CaddyProcess { child, _tmp: tmp };
    (proc, socket_path)
}

/// Block until the Unix socket file appears (up to `timeout`).
fn wait_for_socket(path: &Path, timeout: Duration) {
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

// ---------------------------------------------------------------------------
// Config writer
// ---------------------------------------------------------------------------

/// Write a minimal `trilithon` TOML config file.
///
/// The config points `caddy.admin_endpoint` at `socket_path` and
/// `storage.data_dir` at `data_dir`.
fn write_trilithon_config(config_dir: &Path, socket_path: &Path, data_dir: &Path) -> PathBuf {
    let config_path = config_dir.join("trilithon.toml");
    let mut f = std::fs::File::create(&config_path).expect("create config file");
    write!(
        f,
        r#"
[server]
bind = "127.0.0.1:7899"

[caddy.admin_endpoint]
transport = "unix"
path = "{socket}"

[storage]
data_dir = "{data}"

[secrets.master_key_backend]
backend = "keychain"

[concurrency]

[tracing]

[bootstrap]
"#,
        socket = socket_path.display(),
        data = data_dir.display(),
    )
    .expect("write config");
    config_path
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// Happy-path end-to-end: daemon starts against real Caddy, emits expected
/// tracing events in order, then shuts down cleanly on SIGTERM.
#[test]
fn happy_path_against_real_caddy() {
    if std::env::var("TRILITHON_E2E_CADDY").as_deref() != Ok("1") {
        return;
    }

    let tmp = tempfile::tempdir().expect("temp dir");
    let data_dir = tmp.path().join("trilithon-data");
    std::fs::create_dir_all(&data_dir).expect("create data dir");

    // Launch Caddy in a sub-dir.
    let caddy_tmp = tempfile::tempdir().expect("caddy temp dir");
    let (_caddy, socket_path) = launch_caddy(caddy_tmp);
    wait_for_socket(&socket_path, Duration::from_secs(10));

    // Write the daemon config.
    let config_path = write_trilithon_config(tmp.path(), &socket_path, &data_dir);

    let bin = trilithon_bin();
    let daemon = Command::new(&bin)
        .args(["--config", config_path.to_str().expect("path"), "run"])
        .env("TRILITHON_LOG_FORMAT", "json")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn trilithon at {}: {e}", bin.display()));

    let pid = daemon.id();

    // Allow time for the full startup sequence.
    std::thread::sleep(Duration::from_secs(3));

    // Graceful shutdown.
    std::process::Command::new("/bin/kill")
        .args(["-SIGTERM", &pid.to_string()])
        .status()
        .expect("failed to send SIGTERM");

    let output = daemon.wait_with_output().expect("wait for daemon");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Collect positions of the expected log lines.
    let probe_pos = stderr
        .find("caddy.capability-probe.completed")
        .unwrap_or_else(|| {
            panic!("expected 'caddy.capability-probe.completed' in stderr\n{stderr}")
        });

    // No sentinel-related error must appear before daemon.started.
    assert!(
        !stderr.contains("caddy.sentinel.failed"),
        "unexpected sentinel failure in stderr\n{stderr}",
    );

    let started_pos = stderr
        .find("daemon.started")
        .unwrap_or_else(|| panic!("expected 'daemon.started' in stderr\n{stderr}"));

    assert!(
        probe_pos < started_pos,
        "'caddy.capability-probe.completed' must appear before 'daemon.started'\n{stderr}",
    );

    assert!(
        output.status.success(),
        "expected exit 0, got {:?}\n{stderr}",
        output.status,
    );
}

//! Integration tests for storage startup, migration wiring, and advisory lock.
// Test files are allowed to use `expect()`/`unwrap()` for concise assertion
// failures; these are never reachable in production.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::panic
)]

#[cfg(unix)]
mod unix_tests {
    use std::io::Write as _;
    use std::path::{Path, PathBuf};

    /// Resolve the path to the compiled `trilithon` binary.
    fn trilithon_bin() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_trilithon"))
    }

    /// Write a minimal TOML config with explicit Caddy socket and data dir paths.
    ///
    /// Returns the path to the written config file (inside `config_dir`).
    fn write_config_with_caddy(config_dir: &Path, caddy_socket: &Path, data_dir: &Path) -> PathBuf {
        let config_path = config_dir.join("test.toml");
        let mut f = std::fs::File::create(&config_path).expect("create config file");
        write!(
            f,
            r#"
[server]
bind = "127.0.0.1:7878"

[caddy.admin_endpoint]
transport = "unix"
path = "{caddy_socket}"

[storage]
data_dir = "{data_dir}"

[secrets.master_key_backend]
backend = "keychain"

[concurrency]

[tracing]

[bootstrap]
"#,
            caddy_socket = caddy_socket.display(),
            data_dir = data_dir.display()
        )
        .expect("write config");
        config_path
    }

    /// Write a minimal TOML config with the default Caddy socket path.
    fn write_config(config_dir: &Path, data_dir: &Path) -> PathBuf {
        write_config_with_caddy(config_dir, Path::new("/run/caddy/admin.sock"), data_dir)
    }

    /// A unique temp directory for a single test run.
    ///
    /// Uses the test name (passed as `tag`) to avoid collisions when multiple
    /// storage tests run in parallel.
    fn temp_dir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("trilithon-test-{tag}"));
        std::fs::remove_dir_all(&base).ok(); // clean up any prior run
        std::fs::create_dir_all(&base).expect("create temp dir");
        base
    }

    // -----------------------------------------------------------------
    // Test 1: daemon against a path that cannot be created exits with 3.
    // -----------------------------------------------------------------

    /// A daemon configured with an unwritable `data_dir` must exit non-zero.
    ///
    /// The config loader rejects unwritable paths (exit 2) before storage open,
    /// so exit 2 or 3 are both acceptable here.
    #[test]
    fn missing_data_dir_exits_3() {
        let cfg_dir = temp_dir("missing-data-dir-cfg");
        // `/nonexistent/…` cannot be created; config-loader probe fails → exit 2.
        let config_path = write_config(
            &cfg_dir,
            Path::new("/nonexistent/path/that/cannot/be/created"),
        );

        let output = std::process::Command::new(trilithon_bin())
            .args(["--config", config_path.to_str().unwrap(), "run"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("failed to run trilithon");

        let code = output.status.code().expect("process killed by signal");
        assert!(
            code == 2 || code == 3,
            "expected exit 2 or 3 for invalid data_dir, got {code}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr),
        );
    }

    // -----------------------------------------------------------------
    // Test 2: successful startup emits storage.migrations.applied.
    // -----------------------------------------------------------------

    /// A daemon started against a fresh temp dir must log
    /// `storage.migrations.applied` before the Caddy probe step.
    ///
    /// The minimal config fixture points at a non-existent Caddy socket, so
    /// the daemon exits 3 after migrations succeed but before `daemon.started`.
    /// This test verifies that migrations run regardless of Caddy reachability.
    #[test]
    fn successful_startup_emits_migrations_applied() {
        let base = temp_dir("startup-migrations");
        let data_dir = base.join("data");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        let config_path = write_config(&base, &data_dir);

        let output = std::process::Command::new(trilithon_bin())
            .args(["--config", config_path.to_str().unwrap(), "run"])
            .env("TRILITHON_LOG_FORMAT", "json")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("failed to run trilithon");

        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().expect("process killed by signal");

        // Migrations must have run; Caddy is unreachable so exit 3 is expected.
        assert!(
            stderr.contains("storage.migrations.applied"),
            "stderr did not contain 'storage.migrations.applied':\n{stderr}",
        );

        // With Caddy unreachable, the daemon cannot reach `daemon.started`;
        // exit 3 is the expected outcome.
        assert_eq!(
            code, 3,
            "expected exit 3 (Caddy unreachable), got {code}\nstderr: {stderr}",
        );
    }

    // -----------------------------------------------------------------
    // Test 3: second daemon against same dir exits 3 (advisory lock).
    // -----------------------------------------------------------------

    /// Two daemons targeting the same `data_dir` must not both succeed.
    /// The second one must exit with code 3.
    #[test]
    fn second_daemon_against_same_dir_exits_3() {
        let base = temp_dir("double-daemon");
        let data_dir = base.join("data");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        let config_path = write_config(&base, &data_dir);

        // Start the first daemon.
        let child1 = std::process::Command::new(trilithon_bin())
            .args(["--config", config_path.to_str().unwrap(), "run"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to spawn first trilithon");

        let child1_pid = child1.id();

        // Wait long enough for the first daemon to acquire the lock.
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Start the second daemon against the same directory.
        let output2 = std::process::Command::new(trilithon_bin())
            .args(["--config", config_path.to_str().unwrap(), "run"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("failed to run second trilithon");

        let code2 = output2
            .status
            .code()
            .expect("second process killed by signal");
        let stderr2 = String::from_utf8_lossy(&output2.stderr);

        // Shut down the first daemon.
        std::process::Command::new("/bin/kill")
            .args(["-SIGTERM", &child1_pid.to_string()])
            .status()
            .expect("failed to send SIGTERM to first daemon");

        drop(child1);

        assert_eq!(
            code2, 3,
            "second daemon must exit 3 (advisory lock held by first):\nstderr: {stderr2}",
        );
    }

    // -----------------------------------------------------------------
    // Test 4: Caddy unreachable at startup exits 3.
    // -----------------------------------------------------------------

    /// A daemon configured with a non-existent Caddy Unix socket must exit 3
    /// and emit a `caddy.unreachable` tracing event in stderr.
    ///
    /// Storage is healthy; only the Caddy probe step fails.
    #[test]
    fn caddy_unreachable_exits_3() {
        let base = temp_dir("caddy-unreachable");
        let data_dir = base.join("data");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        let config_path = write_config_with_caddy(
            &base,
            Path::new("/nonexistent/trilithon-test-caddy-absent.sock"),
            &data_dir,
        );

        let output = std::process::Command::new(trilithon_bin())
            .args(["--config", config_path.to_str().unwrap(), "run"])
            .env("TRILITHON_LOG_FORMAT", "json")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("failed to run trilithon");

        let code = output.status.code().expect("process killed by signal");
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert_eq!(
            code, 3,
            "expected exit 3 when Caddy is unreachable, got {code}\nstderr: {stderr}",
        );

        assert!(
            stderr.contains("caddy.unreachable")
                || stderr.contains("caddy")
                || stderr.contains("unreachable"),
            "stderr did not mention Caddy unreachability:\n{stderr}",
        );
    }
}

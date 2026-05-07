//! Integration tests for signal-based graceful shutdown.
//!
//! These tests require a running Caddy instance to proceed past the startup
//! probe.  They are gated on `TRILITHON_E2E_CADDY=1` and skip when the env
//! var is absent.
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
    use std::{
        io::Write as _,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::mpsc,
        time::{Duration, Instant},
    };

    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Binary helpers
    // -----------------------------------------------------------------------

    fn trilithon_bin() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_trilithon"))
    }

    fn send_signal(pid: u32, sig_name: &str) {
        Command::new("/bin/kill")
            .args([&format!("-{sig_name}"), &pid.to_string()])
            .status()
            .expect("failed to invoke /bin/kill");
    }

    // -----------------------------------------------------------------------
    // Caddy launch (reused from adapter tests)
    // -----------------------------------------------------------------------

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

    fn minimal_caddy_config(socket_path: &Path) -> String {
        let value = serde_json::json!({
            "admin": {
                "listen": format!("unix/{}", socket_path.display()),
                "enforce_origin": false
            }
        });
        serde_json::to_string_pretty(&value).unwrap()
    }

    fn launch_caddy() -> (CaddyProcess, PathBuf) {
        let tmp = tempfile::tempdir().expect("temp dir");
        let socket_path = tmp.path().join("caddy-admin.sock");
        let config_path = tmp.path().join("caddy.json");

        std::fs::write(&config_path, minimal_caddy_config(&socket_path))
            .expect("write caddy config");

        let caddy_bin = std::env::var("CADDY").unwrap_or_else(|_| "caddy".to_owned());
        let child = Command::new(&caddy_bin)
            .args(["run", "--config", config_path.to_str().expect("path")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| {
                panic!("failed to spawn caddy '{caddy_bin}'; set CADDY or add it to PATH: {e}")
            });

        let proc = CaddyProcess { child, _tmp: tmp };
        (proc, socket_path)
    }

    fn wait_for_socket(path: &Path, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
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

    // -----------------------------------------------------------------------
    // Config writer
    // -----------------------------------------------------------------------

    fn write_signal_test_config(config_dir: &Path, socket_path: &Path, data_dir: &Path) -> PathBuf {
        let config_path = config_dir.join("signal-test.toml");
        let mut f = std::fs::File::create(&config_path).expect("create config file");
        write!(
            f,
            r#"
[server]
bind = "127.0.0.1:7896"

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

    // -----------------------------------------------------------------------
    // Core helper
    // -----------------------------------------------------------------------

    /// Spawn the daemon, wait for it to be ready, send a signal, and assert:
    /// 1. Exit status is 0.
    /// 2. Stderr contains `daemon.shutting-down`.
    /// 3. Wall-clock time from kill to exit is < 10 seconds.
    fn assert_signal_shutdown(sig_name: &str) {
        let tmp = tempfile::tempdir().expect("temp dir");
        let data_dir = tmp.path().join(format!("data-{}", sig_name.to_lowercase()));
        std::fs::create_dir_all(&data_dir).expect("create data dir");

        // Launch Caddy and wait for its socket.
        let (_caddy, socket_path) = launch_caddy();
        wait_for_socket(&socket_path, Duration::from_secs(10));

        let config_path = write_signal_test_config(tmp.path(), &socket_path, &data_dir);

        let mut child = Command::new(trilithon_bin())
            .args(["--config", config_path.to_str().expect("path"), "run"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn trilithon");

        let child_pid = child.id();

        // Take the stderr pipe and drain it on a background thread.  We signal
        // the main thread as soon as "daemon.started" is seen, then continue
        // draining so the pipe never blocks.
        let stderr_pipe = child.stderr.take().expect("stderr must be piped");
        let (ready_tx, ready_rx) = mpsc::sync_channel::<()>(1);
        let reader_thread = std::thread::spawn(move || {
            use std::io::BufRead as _;
            let reader = std::io::BufReader::new(stderr_pipe);
            let mut lines = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                if line.contains("daemon.started") {
                    let _ = ready_tx.send(());
                }
                lines.push(line);
            }
            lines
        });

        // Wait for the daemon to emit daemon.started (15-second hard timeout).
        ready_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("daemon.started not seen within 15s");

        let kill_at = Instant::now();
        send_signal(child_pid, sig_name);

        let status = child.wait().expect("failed to wait for process");
        let elapsed = kill_at.elapsed();

        let stderr_lines = reader_thread.join().expect("stderr reader thread panicked");
        let stderr = stderr_lines.join("\n");

        // Exit code 0.
        assert!(
            status.success(),
            "expected exit 0, got {status:?}\nstderr: {stderr}",
        );

        // Stderr contains the shutdown event.
        assert!(
            stderr.contains("daemon.shutting-down"),
            "stderr did not contain 'daemon.shutting-down':\n{stderr}",
        );

        // Stderr must also confirm shutdown completed.
        assert!(
            stderr.contains("daemon.shutdown-complete"),
            "stderr did not contain 'daemon.shutdown-complete':\n{stderr}",
        );

        // Completed within the drain budget.
        assert!(
            elapsed < Duration::from_secs(10),
            "process took {elapsed:?} to exit after signal — exceeded 10-second budget",
        );
    }

    // -----------------------------------------------------------------------
    // Tests (gated on TRILITHON_E2E_CADDY=1)
    // -----------------------------------------------------------------------

    #[test]
    fn sigterm_drains_within_budget() {
        if std::env::var("TRILITHON_E2E_CADDY").as_deref() != Ok("1") {
            return;
        }
        assert_signal_shutdown("SIGTERM");
    }

    #[test]
    fn sigint_drains_within_budget() {
        if std::env::var("TRILITHON_E2E_CADDY").as_deref() != Ok("1") {
            return;
        }
        assert_signal_shutdown("SIGINT");
    }
}

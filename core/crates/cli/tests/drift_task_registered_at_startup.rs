//! Slice 8.5 — verifies that the drift-detection task is registered during
//! daemon startup and that `init_from_storage` runs without error.
//!
//! Requires a live Caddy instance (gated on `TRILITHON_E2E_CADDY=1`) because
//! the drift detector is spawned after the Caddy reachability probe.
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

    fn trilithon_bin() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_trilithon"))
    }

    fn send_sigterm(pid: u32) {
        Command::new("/bin/kill")
            .args(["-SIGTERM", &pid.to_string()])
            .status()
            .expect("failed to invoke /bin/kill");
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

    fn write_config(config_dir: &Path, socket_path: &Path, data_dir: &Path) -> PathBuf {
        let config_path = config_dir.join("drift-startup-test.toml");
        let mut f = std::fs::File::create(&config_path).expect("create config file");
        write!(
            f,
            r#"
[server]
bind = "127.0.0.1:7897"

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

    /// Verify that the drift-detection task is registered at startup.
    ///
    /// The test launches the daemon against a live Caddy instance, waits for
    /// `daemon.started` (which is emitted only after the drift task is spawned),
    /// and checks that no `drift-detector.init-from-storage-failed` error appears
    /// in the logs.
    #[test]
    fn drift_task_registered_at_startup() {
        if std::env::var("TRILITHON_E2E_CADDY").as_deref() != Ok("1") {
            return;
        }

        let tmp = tempfile::tempdir().expect("temp dir");
        let data_dir = tmp.path().join("data");
        std::fs::create_dir_all(&data_dir).expect("create data dir");

        let (_caddy, socket_path) = launch_caddy();
        wait_for_socket(&socket_path, Duration::from_secs(10));

        let config_path = write_config(tmp.path(), &socket_path, &data_dir);

        let mut child = Command::new(trilithon_bin())
            .args(["--config", config_path.to_str().expect("path"), "run"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn trilithon");

        let child_pid = child.id();

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

        // daemon.started is emitted only after the drift task is spawned.
        ready_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("daemon.started not seen within 15s — drift task may not have been registered");

        send_sigterm(child_pid);
        child.wait().expect("failed to wait for process");

        let stderr_lines = reader_thread.join().expect("stderr reader thread panicked");
        let stderr = stderr_lines.join("\n");

        // The drift detector must not log an init-from-storage error on a fresh DB.
        assert!(
            !stderr.contains("drift-detector.init-from-storage-failed"),
            "drift-detector.init-from-storage-failed found in logs — \
             init_from_storage errored on startup:\n{stderr}",
        );
    }
}

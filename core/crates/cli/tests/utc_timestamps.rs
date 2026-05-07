//! Integration test asserting that daemon log events carry UTC `ts_unix_seconds`
//! integer fields within 60 seconds of wall-clock time.
// Test files are allowed to use `expect()`/`unwrap()` for concise assertion
// failures; these are never reachable in production.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

#[cfg(unix)]
mod unix_tests {
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::SystemTime;

    /// Path to the minimal config fixture.
    fn fixture_config() -> &'static Path {
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/minimal.toml"
        ))
    }

    /// Resolve the path to the compiled `trilithon` binary.
    fn trilithon_bin() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_trilithon"))
    }

    /// Current time as a Unix timestamp in seconds, cast safely to i64.
    ///
    /// Panics only if the system clock is set before the UNIX epoch.
    fn now_unix_secs() -> i64 {
        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_secs();
        // i64::MAX in seconds is year ~2262, safe for the foreseeable future.
        i64::try_from(secs).expect("unix timestamp overflows i64")
    }

    #[test]
    fn log_events_have_utc_unix_seconds() {
        // Capture the wall-clock range around the test.
        let before = now_unix_secs();

        // Use a unique data directory so this test does not conflict with
        // other tests that run the daemon in parallel.
        let data_dir = std::env::temp_dir().join("trilithon-utc-timestamps-test");
        std::fs::create_dir_all(&data_dir).expect("create test data dir");

        let mut child = std::process::Command::new(trilithon_bin())
            .args([
                "--config",
                fixture_config().to_str().expect("utf-8 path"),
                "run",
            ])
            // Force JSON log format so we can parse ts_unix_seconds.
            .env("TRILITHON_LOG_FORMAT", "json")
            .env(
                "TRILITHON_STORAGE__DATA_DIR",
                data_dir.to_str().expect("utf-8 path"),
            )
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to spawn trilithon");

        let child_pid = child.id();

        // Drain stderr on a background thread to prevent pipe-buffer deadlock
        // if output exceeds the OS pipe buffer (typically 64 KB).
        let stderr_pipe = child.stderr.take().expect("stderr must be piped");
        let (ready_tx, ready_rx) = mpsc::sync_channel::<()>(1);
        let reader_thread = std::thread::spawn(move || {
            use std::io::BufRead as _;
            let reader = std::io::BufReader::new(stderr_pipe);
            let mut lines = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                // Signal readiness on first JSON line (daemon has started logging).
                // try_send avoids blocking when the channel is already full
                // (the daemon emits several JSON lines before we consume the first).
                if line.trim_start().starts_with('{') {
                    let _ = ready_tx.try_send(());
                }
                lines.push(line);
            }
            lines
        });

        // Wait for the first JSON log line (up to 10 s), then give it another
        // moment to emit more lines before sending SIGTERM.
        let _ = ready_rx.recv_timeout(std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Send SIGTERM to trigger graceful shutdown.
        std::process::Command::new("/bin/kill")
            .args(["-SIGTERM", &child_pid.to_string()])
            .status()
            .expect("failed to invoke /bin/kill");

        let status = child.wait().expect("failed to wait for trilithon");
        let after = now_unix_secs();

        let stderr_lines = reader_thread.join().expect("stderr reader thread panicked");
        let stderr = stderr_lines.join("\n");

        // Accept any exit status: exit 0 (clean shutdown) and exit 3
        // (Caddy unavailable in test environment) are both valid — the test
        // only asserts that whatever log lines were emitted carry correct
        // ts_unix_seconds timestamps, not that the daemon ran a full lifecycle.
        let _ = status;

        // Parse stderr as JSON lines and assert ts_unix_seconds fields.
        let mut checked = 0usize;

        for line in stderr.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Non-JSON lines (e.g. the pre-tracing line) are tolerated.
            let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                continue;
            };
            let Some(ts) = value
                .get("ts_unix_seconds")
                .and_then(serde_json::Value::as_i64)
            else {
                continue;
            };
            // Allow up to 60 s of drift to cover any slow CI host.
            assert!(
                ts >= before - 60 && ts <= after + 60,
                "ts_unix_seconds {ts} is not within 60 s of [{before}, {after}]",
            );
            checked += 1;
        }

        assert!(
            checked > 0,
            "no JSON log lines with ts_unix_seconds found in stderr:\n{stderr}",
        );
    }
}

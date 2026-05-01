//! Integration test asserting that daemon log events carry UTC `ts_unix_seconds`
//! integer fields within 60 seconds of wall-clock time.
// Test files are allowed to use `expect()`/`unwrap()` for concise assertion
// failures; these are never reachable in production.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

#[cfg(unix)]
mod unix_tests {
    use std::path::Path;
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

        let child = std::process::Command::new(trilithon_bin())
            .args([
                "--config",
                fixture_config().to_str().expect("utf-8 path"),
                "run",
            ])
            // Force JSON log format so we can parse ts_unix_seconds.
            .env("TRILITHON_LOG_FORMAT", "json")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to spawn trilithon");

        let child_pid = child.id();

        // Give the process a moment to initialise its Tokio runtime and emit
        // at least one structured log event.
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Send SIGTERM to trigger graceful shutdown.
        std::process::Command::new("/bin/kill")
            .args(["-SIGTERM", &child_pid.to_string()])
            .status()
            .expect("failed to invoke /bin/kill");

        let output = child
            .wait_with_output()
            .expect("failed to wait for trilithon");

        let after = now_unix_secs();

        // Parse stderr as JSON lines and assert ts_unix_seconds fields.
        let stderr = String::from_utf8_lossy(&output.stderr);
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

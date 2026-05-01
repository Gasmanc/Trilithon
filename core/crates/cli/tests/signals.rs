//! Integration tests for signal-based graceful shutdown.
// Test files are allowed to use `expect()`/`unwrap()` for concise assertion
// failures; these are never reachable in production.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

#[cfg(unix)]
mod unix_tests {
    use std::path::Path;
    use std::time::Instant;

    /// Path to the minimal config fixture shared across signal tests.
    fn fixture_config() -> &'static Path {
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/minimal.toml"
        ))
    }

    /// Resolve the path to the compiled `trilithon` binary.
    ///
    /// `CARGO_BIN_EXE_trilithon` is set by Cargo when running integration tests
    /// and points directly to the freshly-built binary.
    fn trilithon_bin() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_trilithon"))
    }

    /// Send `sig_name` (e.g. `"SIGTERM"`) to `pid` via `/bin/kill`.
    ///
    /// We use `/bin/kill` rather than the `nix` crate to avoid any interaction
    /// with Tokio's internal signal-pipe setup in the test runner.  `/bin/kill`
    /// goes through the kernel's native signal delivery path.
    fn send_signal(pid: u32, sig_name: &str) {
        std::process::Command::new("/bin/kill")
            .args([&format!("-{sig_name}"), &pid.to_string()])
            .status()
            .expect("failed to invoke /bin/kill");
    }

    /// Spawn the daemon, wait for it to be ready, send a signal, and assert:
    /// 1. Exit status is 0.
    /// 2. Stderr contains `daemon.shutting-down`.
    /// 3. Wall-clock time from kill to exit is < 10 seconds.
    fn assert_signal_shutdown(sig_name: &str) {
        let child = std::process::Command::new(trilithon_bin())
            .args([
                "--config",
                fixture_config().to_str().expect("utf-8 path"),
                "run",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to spawn trilithon");

        let child_pid = child.id();

        // Give the process a moment to initialise its Tokio runtime and
        // signal handlers.  1 second is generous but keeps the test well
        // within the 10-second drain budget.
        std::thread::sleep(std::time::Duration::from_secs(1));

        let kill_at = Instant::now();
        send_signal(child_pid, sig_name);

        let output = child
            .wait_with_output()
            .expect("failed to wait for process");
        let elapsed = kill_at.elapsed();

        // Exit code 0.
        assert!(
            output.status.success(),
            "expected exit 0, got {:?}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );

        // Stderr contains the shutdown event.
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("daemon.shutting-down"),
            "stderr did not contain 'daemon.shutting-down':\n{stderr}",
        );

        // Completed within the drain budget.
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "process took {elapsed:?} to exit after signal — exceeded 10-second budget",
        );
    }

    #[test]
    fn sigterm_drains_within_budget() {
        assert_signal_shutdown("SIGTERM");
    }

    #[test]
    fn sigint_drains_within_budget() {
        assert_signal_shutdown("SIGINT");
    }
}

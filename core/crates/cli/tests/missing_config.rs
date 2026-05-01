//! Integration tests for missing/malformed configuration exit codes.
// Test files are allowed to use `expect()`/`unwrap()` for concise assertion
// failures; these are never reachable in production.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn missing_config_exits_2() {
    let mut cmd = Command::cargo_bin("trilithon").unwrap();
    cmd.args(["--config", "/nonexistent/path/config.toml", "run"])
        .assert()
        .code(2)
        .stderr(contains("configuration file not found"));
}

#[test]
fn malformed_config_exits_2() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/malformed.toml");
    let mut cmd = Command::cargo_bin("trilithon").unwrap();
    cmd.args(["--config", fixture, "run"])
        .assert()
        .code(2)
        .stderr(contains("malformed TOML"));
}

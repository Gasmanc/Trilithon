//! Integration test: `--allow-remote-admin` exits 2 with the documented message.
// Test files are allowed to use `expect()`/`unwrap()` for concise assertion
// failures; these are never reachable in production.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn flag_exits_2() {
    let mut cmd = Command::cargo_bin("trilithon").unwrap();
    cmd.args(["--allow-remote-admin", "run"])
        .assert()
        .code(2)
        .stderr(contains(
            "--allow-remote-admin is OUT OF SCOPE FOR V1; remove the flag and rerun.",
        ));
}

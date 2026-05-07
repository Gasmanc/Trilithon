//! Integration tests for `config show`.
// Test files are allowed to use `expect()`/`unwrap()` for concise assertion
// style and insta macros that call unwrap internally.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

use assert_cmd::Command;

#[test]
fn shows_redacted() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let data_dir = tmp.path().to_str().expect("UTF-8 path").to_owned();

    let output = Command::cargo_bin("trilithon")?
        .args([
            "--config",
            "tests/fixtures/with_secrets.toml",
            "config",
            "show",
        ])
        .env("TRILITHON_STORAGE__DATA_DIR", &data_dir)
        .env_remove("TRILITHON_GIT_SHORT_HASH")
        .env_remove("TRILITHON_RUSTC_VERSION")
        .output()?;

    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout)?;
    let normalized = stdout.replace(&data_dir, "[TEMP_DATA_DIR]");
    insta::assert_snapshot!(normalized);
    assert!(stdout.contains("***"), "expected *** in output");
    assert!(
        !stdout.contains("/etc/trilithon/secret-creds.json"),
        "output must not contain the secret path"
    );
    Ok(())
}

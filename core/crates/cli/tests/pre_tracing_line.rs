//! Integration test: the very first line on stderr is the pre-tracing sentinel.

#[test]
fn pre_tracing_line_is_first_on_stderr() -> Result<(), Box<dyn std::error::Error>> {
    // Use `run` — the pre-tracing line is only emitted for daemon paths, not for
    // fast-exit commands like `version` or `config show`. The command will fail
    // quickly (bad data_dir) but the first stderr line must be the sentinel.
    let tmp = tempfile::tempdir()?;
    let mut cmd = assert_cmd::Command::cargo_bin("trilithon")?;
    cmd.args(["--config", "tests/fixtures/minimal.toml", "run"])
        .env("TRILITHON_STORAGE__DATA_DIR", tmp.path());
    let output = cmd.output()?;
    let stderr = String::from_utf8(output.stderr)?;
    let first_line = stderr.lines().next().unwrap_or("");
    assert_eq!(
        first_line, "trilithon: starting (pre-tracing)",
        "first stderr line was not the pre-tracing sentinel.\nFull stderr:\n{stderr}"
    );
    Ok(())
}

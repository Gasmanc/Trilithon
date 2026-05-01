//! Integration test: the very first line on stderr is the pre-tracing sentinel.

use assert_cmd::Command;

#[test]
fn pre_tracing_line_is_first_on_stderr() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("trilithon")?;
    cmd.arg("run");
    // `run` exits with success (placeholder) and writes the pre-tracing line.
    let output = cmd.output()?;
    let stderr = String::from_utf8(output.stderr)?;
    let first_line = stderr.lines().next().unwrap_or("");
    assert_eq!(
        first_line, "trilithon: starting (pre-tracing)",
        "first stderr line was not the pre-tracing sentinel.\nFull stderr:\n{stderr}"
    );
    Ok(())
}

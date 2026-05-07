//! Integration test: `trilithon version` prints the expected one-line format.

use assert_cmd::Command;

#[test]
fn version_line_format() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("trilithon")?;
    cmd.arg("version");
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone())?;
    let line = stdout.trim();
    // Expected: `trilithon <semver> (<12-hex-char hash>) rustc <version>...`
    let re =
        regex::Regex::new(r"^trilithon \d+\.\d+\.\d+ \([0-9a-f]{12}\) rustc \d+\.\d+\.\d+.*$")?;
    assert!(
        re.is_match(line),
        "version line did not match expected pattern.\nGot: {line:?}"
    );
    Ok(())
}

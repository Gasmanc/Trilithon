//! Build script for the `trilithon` CLI binary.
//!
//! Emits `TRILITHON_GIT_SHORT_HASH` and `TRILITHON_RUSTC_VERSION` as
//! `cargo:rustc-env` variables so that `main.rs` can embed them at compile
//! time.

fn main() {
    // Only re-run this script when the git HEAD actually changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads/");

    let git = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".into(), |s| s.trim().to_string());
    println!("cargo:rustc-env=TRILITHON_GIT_SHORT_HASH={git}");

    let rustc_bin = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let rustc = std::process::Command::new(&rustc_bin)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "rustc-unknown".into(), |s| s.trim().to_string());
    println!("cargo:rustc-env=TRILITHON_RUSTC_VERSION={rustc}");
}

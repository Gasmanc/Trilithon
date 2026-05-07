//! Build script for the `trilithon` CLI binary.
//!
//! Emits `TRILITHON_GIT_SHORT_HASH` and `TRILITHON_RUSTC_VERSION` as
//! `cargo:rustc-env` variables so that `main.rs` can embed them at compile
//! time.

fn main() {
    // Resolve the workspace .git directory relative to CARGO_MANIFEST_DIR so
    // that `rerun-if-changed` points at the real files (the naive relative path
    // would resolve to core/crates/cli/.git/HEAD, which never exists).
    let manifest =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let git_root = manifest.join("../../../.git");
    println!("cargo:rerun-if-changed={}", git_root.join("HEAD").display());
    println!(
        "cargo:rerun-if-changed={}",
        git_root.join("refs/heads/").display()
    );

    let git = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(
            || {
                println!(
                    "cargo:warning=git not available; TRILITHON_GIT_SHORT_HASH will be 'unknown'"
                );
                "unknown".into()
            },
            |s| s.trim().to_string(),
        );
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

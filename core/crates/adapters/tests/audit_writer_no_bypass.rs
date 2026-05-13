//! `AuditWriter` bypass guard.
//!
//! This test walks every `.rs` source file in the `trilithon-adapters` crate
//! and asserts that `.record_audit_event(` (a method *call*) only appears in
//! `audit_writer.rs`.
//!
//! Trait *implementations* (`fn record_audit_event`) are explicitly allowed
//! because `SqliteStorage` and test doubles must define the method. The guard
//! targets callers, not implementors.
//!
//! The rule it enforces: no adapter may call `Storage::record_audit_event`
//! directly; all audit writes MUST go through `AuditWriter::record`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::path::Path;

/// The pattern that signals a *call* to the method (not a definition).
///
/// A definition looks like `fn record_audit_event`; a call looks like
/// `.record_audit_event(`. We match the call form only.
const CALL_PATTERN: &str = ".record_audit_event(";

/// Source file stems that are permitted to contain the call pattern.
///
/// Production callers: the writer itself only.  Test files are blanket-allowed
/// by directory (see [`is_test_path`] below); stems remain for the writer file
/// and for legacy compatibility with prior fixed-list reviewers.
///
/// - `audit_writer` — the writer itself; only legitimate production caller.
const ALLOWED_CALL_STEMS: &[&str] = &["audit_writer"];

/// Whether `path` lives under a `tests/` directory anywhere in its ancestry
/// (or is itself a `tests` subtree).  Used as the structural allowlist
/// predicate so adding a new test file does not require touching this guard.
fn is_test_path(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "tests")
}

fn collect_rs_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(collect_rs_files(&path));
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                result.push(path);
            }
        }
    }
    result
}

#[test]
fn no_direct_record_audit_event_outside_audit_writer() {
    // CARGO_MANIFEST_DIR is set by Cargo for integration tests.
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by Cargo");

    let crate_root = Path::new(&manifest_dir);
    let src_root = crate_root.join("src");
    let tests_root = crate_root.join("tests");
    // The cli crate is one level up from the adapters crate root.
    let cli_src_root = crate_root.join("../../crates/cli/src");

    let mut violations: Vec<String> = Vec::new();

    for dir in [&src_root, &tests_root, &cli_src_root] {
        for path in collect_rs_files(dir) {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            // Test files anywhere under a `tests/` directory may call
            // `record_audit_event` directly — they are not production paths.
            if is_test_path(&path) {
                continue;
            }

            // Explicitly-allowed production stems (currently only the writer).
            if ALLOWED_CALL_STEMS.contains(&stem) {
                continue;
            }

            let content = std::fs::read_to_string(&path).unwrap_or_default();

            if content.contains(CALL_PATTERN) {
                violations.push(format!(
                    "  {} — calls `{CALL_PATTERN}` outside `audit_writer.rs`",
                    path.display()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Bypass guard failed. The following files call `record_audit_event` \
        directly, bypassing `AuditWriter::record`:\n{}",
        violations.join("\n")
    );
}

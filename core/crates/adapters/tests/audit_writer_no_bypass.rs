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
/// - `audit_writer` — the writer itself; only legitimate production caller.
/// - `audit_writer_no_bypass` — this test file.
/// - `audit_kind_validation`, `sqlite_storage` — pre-existing low-level
///   storage contract tests that bypass the writer intentionally to verify
///   that the storage layer enforces its own invariants (kind validation,
///   immutability triggers, etc.).  These do not violate the guard's intent
///   because they are testing storage internals, not production write paths.
/// - `audit_query_*` — Slice 6.6 query tests that seed rows directly to
///   exercise the `tail_audit_log` storage method; inserting via the writer
///   would couple these tests to `AuditWriter` internals unnecessarily.
const ALLOWED_CALL_STEMS: &[&str] = &[
    "audit_writer",
    "audit_writer_no_bypass",
    "audit_kind_validation",
    "sqlite_storage",
    "audit_query_pagination",
    "audit_query_correlation_filter",
    "audit_query_time_range",
    "audit_query_event_filter",
    "audit_query_actor_filter",
    "audit_query_cursor_descending",
];

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

    let mut violations: Vec<String> = Vec::new();

    for dir in [&src_root, &tests_root] {
        for path in collect_rs_files(dir) {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            // Permitted call sites.
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

# Phase 8 — Security Review Findings

**Reviewer:** security
**Date:** 2026-05-11
**Diff range:** 4402d00..HEAD
**Phase:** 8

---

[WARNING] INSTANCE_ID_UNUSED_IN_SQL
File: core/crates/adapters/src/sqlite_storage.rs
Lines: latest_unresolved_drift_event
Description: instance_id parameter accepted but never used in query.
Suggestion: Add WHERE filter or remove parameter.

[WARNING] DIFF_JSON_STORED_WITHOUT_REDACTION
File: core/crates/adapters/src/drift.rs
Lines: 251-254
Description: Diff JSON stored without passing through secrets redactor. May contain plaintext secrets.
Suggestion: Route through SecretsRedactor before persisting.

[WARNING] BOX_LEAK_MEMORY
File: core/crates/cli/src/run.rs
Lines: 249-252
Description: Box::leak creates permanent heap allocations.
Suggestion: Use Arc instead.

[WARNING] NON_ATOMIC_WRITES
File: core/crates/adapters/src/drift.rs
Lines: 288-333
Description: record() writes audit and drift_events as separate operations without transaction.
Suggestion: Wrap in SQLite transaction or document trade-off.

[SUGGESTION] DEFER_MAPS_TO_ROLLEDBACK
File: core/crates/adapters/src/drift.rs
Lines: 349-353
Description: ResolutionKind::Defer mapped to DriftResolution::RolledBack — misleading audit data.
Suggestion: Add Deferred variant.

[SUGGESTION] REGEX_VERSION_PIN
File: core/Cargo.toml
Lines: regex = "1"
Description: Broad version pin for regex dependency.
Suggestion: No action needed.

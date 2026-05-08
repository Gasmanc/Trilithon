# Codex — Phase 6 Review Findings

**Reviewer:** codex (gpt-5.3-codex)
**Phase:** 6 — Audit log with secrets-aware redactor
**Date:** 2026-05-09

---

[CRITICAL] DIFF_REDACTION_BYPASS_FOR_PHASE8_SHAPE
File: core/crates/adapters/src/audit_writer.rs
Lines: 147-149, 173-175
Description: `AuditWriter` applies `SecretsRedactor::redact` to the entire `diff` value. For the documented diff envelope shape (`{added, removed, modified}`), secret paths are shifted under top-level keys and no longer match schema pointers, so plaintext secrets can be stored in `redacted_diff_json`.
Suggestion: Redact audit diffs with `SecretsRedactor::redact_diff` (or detect and handle diff-envelope shape explicitly) and add an integration test that proves secrets under `added`/`modified` are redacted.

[HIGH] IMMUTABILITY_MIGRATION_FALLBACK_SCHEMA_MISMATCH
File: core/crates/adapters/migrations/0006_audit_immutable.sql
Lines: 9-25
Description: The fallback `CREATE TABLE IF NOT EXISTS audit_log` does not match the canonical table shape (it omits `prev_hash` and `caddy_instance_id`). If this fallback path executes, storage code that reads/writes those columns can fail or violate hash-chain assumptions.
Suggestion: Remove fallback table creation from this migration, or make it byte-for-byte schema-compatible with `0001_init.sql` for `audit_log` (including `prev_hash`, `caddy_instance_id`, and defaults).

[WARNING] IN_MEMORY_CURSOR_PAGINATION_ORDER_DRIFT
File: core/crates/core/src/storage/in_memory.rs
Lines: 235-244
Description: Cursor filtering uses `row.id < cursor_before`, but result ordering is still insertion order (`.rev()`) rather than `id DESC` like SQLite. This can produce pagination behavior in tests/memory mode that diverges from production and can skip/duplicate rows when insertion order and ULID order differ.
Suggestion: Sort filtered rows by `id` descending before `take(limit)` to match `SqliteStorage::tail_audit_log` semantics.

[WARNING] AUDIT_KIND_VOCAB_CARDINALITY_NOT_ASSERTED
File: core/crates/core/src/audit/event.rs
Lines: 110-112
Description: `AuditEvent::kind_str()` is exhaustive but no compile-time assertion verifies that `all_variants().len() == AUDIT_KIND_VOCAB.len()`. If the two diverge, tests may still pass if they don't exercise the specific variant.
Suggestion: Add a test-level assertion that `all_variants().len() == AUDIT_KIND_VOCAB.len()` and that each variant's `kind_str()` appears in `AUDIT_KIND_VOCAB`.

[WARNING] AUDIT_KIND_REGEX_DUPLICATED_IN_ADAPTER
File: core/crates/adapters/src/storage_sqlite/audit.rs
Lines: 33-43
Description: `validate_kind_pattern` manually reimplements the dotted-kind regex check without using `AUDIT_KIND_REGEX` from `core::audit::event`. The two implementations must stay in sync manually.
Suggestion: Export a shared `validate_audit_kind_pattern(kind: &str) -> bool` pure function in core that both can call.

[WARNING] CORRELATION_ID_NOT_CROSS_REFERENCED_AT_WRITE_BOUNDARY
File: core/crates/adapters/src/audit_writer.rs
Lines: 155-170
Description: `AuditWriter::record` accepts `correlation_id: Ulid` from `AuditAppend` and writes it directly to the row, but never cross-references `current_correlation_id()`. A caller could pass a correlation id that differs from the one in the active tracing span, producing an audit trail that doesn't match the actual request lifecycle.
Suggestion: Consider asserting or logging a warning when `append.correlation_id != current_correlation_id()`, or document that callers are responsible for the invariant.

[SUGGESTION] SILENT_NULL_FALLBACK_ON_SERIALIZATION_FAILURE
File: core/crates/adapters/src/audit_writer.rs
Lines: 162
Description: `serde_json::to_string(&redacted).unwrap_or_else(|_| "null".to_owned())` stores the 4-character string `"null"` rather than propagating a serialization error. The immutability trigger means this corruption cannot be corrected.
Suggestion: Either add an `AuditWriteError::Serialization(serde_json::Error)` variant and propagate, or emit a `tracing::error!` so the corruption is observable.

[SUGGESTION] BYPASS_GUARD_MATCHES_ON_FILE_STEM_ONLY
File: core/crates/adapters/tests/audit_writer_no_bypass.rs
Lines: 31-43
Description: `ALLOWED_CALL_STEMS` lists specific test file stems. This is a static allowlist updated manually, fragile to naming drift. If a new test file is added without adding its stem, the guard fires.
Suggestion: Consider expressing the allowlist in terms of directories (`tests/` vs `src/`) rather than file-name stem matching.

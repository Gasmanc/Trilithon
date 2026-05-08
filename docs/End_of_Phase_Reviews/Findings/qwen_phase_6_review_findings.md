# Qwen — Phase 6 Review Findings

**Reviewer:** cc-qwen
**Phase:** 6 — Audit log with secrets-aware redactor
**Date:** 2026-05-09

---

[CRITICAL] SILENT_NULL_ON_SERIALIZATION_FAILURE
File: core/crates/adapters/src/audit_writer.rs
Lines: 160-161
Description: `serde_json::to_string(&redacted)` failure is silently swallowed, storing the literal string `"null"` in `redacted_diff_json`. Downstream consumers querying this row will parse a 4-character string, not a JSON null, and will not detect data loss. The `AuditWriteError` enum has no variant for serialisation failure, so the error is unrecoverable.
Suggestion: Either add an `AuditWriteError::Serialization(serde_json::Error)` variant and propagate, or use `serde_json::to_value` pre-serialisation to fail-fast before the redaction path commits. At minimum, emit a `tracing::error!` so the corruption is observable.

[HIGH] TLS_CORRELATION_ID_NOT_RESTORED_ON_PANIC
File: core/crates/adapters/src/tracing_correlation.rs
Lines: 125-135
Description: `CorrelationSpan::poll` installs `correlation_id` into `CURRENT_CORRELATION_ID`, polls the inner future, then restores the previous value. If the inner future panics during `poll`, the restore line is never reached and the thread-local retains a stale correlation id. Any subsequent code running on that thread will see the wrong correlation id without any warning.
Suggestion: Wrap the poll + restore in a guard struct (RAII pattern) that restores the previous value in `Drop`, ensuring cleanup even on panic.

[HIGH] BYPASS_GUARD_DOES_NOT_COVER_CLI_CRATE
File: core/crates/adapters/tests/audit_writer_no_bypass.rs
Lines: general
Description: The `no_direct_record_audit_event_outside_audit_writer` test scans only `adapters/src/` and `adapters/tests/`. The `cli` crate (where Phase 9 HTTP handlers and background tasks live) is uncovered. A contributor can call `Storage::record_audit_event` directly from `cli`, bypassing the redactor and ULID generation, with no guard catching it.
Suggestion: Extend the bypass guard to scan `cli/src/` as well, or change the approach to a workspace-wide grep.

[HIGH] TWO_PARALLEL_AUDITSELECTOR_TYPES_WITHOUT_CONVERSION
File: core/crates/core/src/audit/row.rs, core/crates/core/src/storage/types.rs
Lines: row.rs:123, types.rs:187
Description: `audit::row::AuditSelector` has `event: Option<AuditEvent>` and `limit`; `storage::types::AuditSelector` has `kind_glob: Option<String>` and no `limit`. No `From`/`Into` exists between them. Phase 9 HTTP handlers will need to translate one to the other; without a verified conversion the `event` filter silently becomes a no-op.
Suggestion: Wire the `audit::row::AuditSelector` as the canonical type through the Storage trait, eliminating the duplicate.

[WARNING] CORRELATION_LAYER_IS_A_NO_OP_STUB
File: core/crates/adapters/src/tracing_correlation.rs
Lines: 161-163
Description: `correlation_layer()` returns `Identity::new()`. Any Phase 9 wiring that attaches it expecting it to stamp spans will silently do nothing; every audit event from HTTP handlers will call `current_correlation_id()`, miss the TLS value, and generate an unrelated fallback ULID.
Suggestion: Return `impl Layer<...>` (opaque return type) so the concrete type can change without breaking callers in Phase 9.

[WARNING] SECRET_FIELD_PATTERNS_FIXED_DEPTH_ONLY
File: core/crates/core/src/schema/secret_fields.rs
Lines: general
Description: `segments_match` requires exact path-length equality. If a future Caddy JSON structure nests secret fields one level deeper than any of the four registered patterns, the redactor passes them through unredacted and the self-check does not catch it.
Suggestion: Audit all Caddy JSON schema fields that can carry secret material and add them. Add a documented review gate for future upstream auth schemes.

[WARNING] NOTES_AND_TARGET_ID_BYPASS_REDACTION
File: core/crates/adapters/src/audit_writer.rs
Lines: 113-114
Description: `AuditAppend.notes` and `target_id` are stored verbatim. A caller that includes a token in an error message placed in `notes`, or uses a secret-bearing string as `target_id`, writes plaintext into the immutable audit log with no detection path.
Suggestion: Add a note in the AuditAppend documentation that callers must not place secret material in `notes` or `target_id`.

[SUGGESTION] AUDIT_KIND_REGEX_UNUSED_IN_PRODUCTION_PATH
File: core/crates/core/src/audit/event.rs
Lines: ~265
Description: `AUDIT_KIND_REGEX` is defined and exported but never used outside test code. The adapter reimplements the pattern manually.
Suggestion: Export a `validate_audit_kind(kind: &str) -> bool` helper in core and use it in both places.

[SUGGESTION] UNUSED_HTTP_AND_TOWER_DEPS_IN_ADAPTERS
File: core/crates/adapters/Cargo.toml
Lines: 24-25
Description: `http` and `tower` are pulled into the adapters crate for the `correlation_layer()` stub that currently returns `Identity`. These deps are unused until Phase 9.
Suggestion: Gate behind a `feature = ["tracing-middleware"]` flag or accept as scaffolding debt until Phase 9.

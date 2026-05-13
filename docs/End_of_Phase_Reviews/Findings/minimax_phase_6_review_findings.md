# Minimax — Phase 6 Review Findings

**Reviewer:** cc-minimax
**Phase:** 6 — Audit log with secrets-aware redactor
**Date:** 2026-05-09

---

[WARNING] SILENT_FALLBACK_ON_REDACTED_DIFF_SERIALIZATION
File: core/crates/adapters/src/audit_writer.rs
Lines: 176
Description: `serde_json::to_string(&redacted).unwrap_or_else(|_| "null".to_owned())` silently replaces a serialization failure with the 4-character string `"null"`. If the redacted Value contained data that cannot be serialized, the "null" string corrupts the audit record without surfacing an error. Per zero-debt rules, production paths must not silently swallow errors.
Suggestion: Propagate the serialization error as `AuditWriteError::SerializationFailed` with a new variant, or at minimum emit `tracing::error!` so the corruption is observable.

[WARNING] BYPASS_GUARD_ALLOWLIST_FRAGILE_TO_NAMING_DRIFT
File: core/crates/adapters/tests/audit_writer_no_bypass.rs
Lines: 31-43
Description: `ALLOWED_CALL_STEMS` lists specific test file stems that may call `record_audit_event` directly. This static allowlist is updated manually. If a new test file is added without its stem, the guard fires — but a production file with a similar stem could be incorrectly allowed.
Suggestion: Express the allowlist in terms of directories (`tests/` vs `src/`) rather than file-name stem matching to make the invariant more robust.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 -->

| # | Finding title | Status | Notes |
|---|--------------|--------|-------|
| 1 | [WARNING] SILENT_FALLBACK_ON_REDACTED_DIFF_SERIALIZATION | SUPERSEDED (dde9dc5) | F003 - Serialization variant shipped |
| 2 | [WARNING] BYPASS_GUARD_ALLOWLIST_FRAGILE | Fixed | F032 - directory-based predicate |

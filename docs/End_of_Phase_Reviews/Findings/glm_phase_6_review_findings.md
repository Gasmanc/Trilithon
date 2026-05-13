# GLM — Phase 6 Review Findings

**Reviewer:** cc-glm
**Phase:** 6 — Audit log with secrets-aware redactor
**Date:** 2026-05-09

---

[HIGH] DEAD_DUAL_TYPE_HIERARCHY_AUDIT_ROW_UNUSED
File: core/crates/core/src/audit/row.rs
Lines: general
Description: Slice 6.2 introduced `AuditEventRow`, `AuditSelector`, `ActorRef`, `AuditOutcome`, `AuditRowId`, and limit constants in `audit::row.rs`. However, all actual wiring in Slices 6.5–6.6 (`audit_writer.rs`, `sqlite_storage.rs`) uses the pre-existing types from `storage::types`. The two `AuditSelector` types are incompatible — storage's uses `kind_glob: Option<String>`, audit/row's uses `event: Option<AuditEvent>`. The `audit::row` types are public dead code that will silently diverge from the actually-used types.
Suggestion: Either wire the `audit::row` types into the storage layer (replacing the `storage::types` versions) or remove the dead `audit::row` types and their re-exports.

[WARNING] READ_SIDE_KIND_VALIDATION_BREAKS_ON_VERSION_ROLLBACK
File: core/crates/adapters/src/sqlite_storage.rs
Lines: ~537
Description: `audit_row_from_sqlite` rejects any row whose `kind` is not in the current `AUDIT_KINDS` list. If a later phase adds a new kind, deploys, writes rows, then rolls back the binary, those rows become unreadable — the audit log is supposed to be immutable and durable, but this gate makes reads version-sensitive.
Suggestion: Remove the kind validation from the read path (it already runs on insert via `validate_kind`). Alternatively, return the row with the unknown kind intact rather than erroring.

[WARNING] CORRELATION_LAYER_COMMITS_NO_OP_RETURN_TYPE
File: core/crates/adapters/src/tracing_correlation.rs
Lines: ~196
Description: `correlation_layer()` returns `tower::layer::util::Identity`, which is a no-op layer. Changing the return type to a concrete `CorrelationIdLayer<S>` in Phase 9 is a breaking API change for any caller that named the type.
Suggestion: Return `impl Layer<...>` (opaque return type) so the concrete type can change without breaking callers.

[WARNING] PHASE_6_FIXED_FILE_HAS_DUPLICATE_FRONTMATTER
File: docs/In_Flight_Reviews/Fixed/phase_6_fixed.md
Lines: 1-63
Description: The file contains three `---`-delimited YAML frontmatter blocks. This is malformed — a markdown file should have at most one frontmatter block.
Suggestion: Merge the three blocks into one canonical frontmatter or split into separate finding files per the one-finding-per-file rule.

[SUGGESTION] AUDIT_KIND_REGEX_DEFINED_BUT_UNUSED
File: core/crates/core/src/audit/event.rs
Lines: ~248
Description: `AUDIT_KIND_REGEX` is exported as a public constant, but nothing in the diff references it. The storage-side kind validation in `storage_sqlite/audit.rs` implements its own manual pattern matching rather than using this regex string.
Suggestion: Either reference it from the validation code or remove it until a consumer exists.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 -->

| # | Finding title | Status | Notes |
|---|--------------|--------|-------|
| 1 | [HIGH] DEAD_DUAL_TYPE_HIERARCHY_AUDIT_ROW_UNUSED | DEFERRED | F006 - Slice 6.2 type-system refactor |
| 2 | [WARNING] READ_SIDE_KIND_VALIDATION_BREAKS_ON_ROLLBACK | Fixed | F016 - read-path vocab check removed |
| 3 | [WARNING] CORRELATION_LAYER_COMMITS_NO_OP_RETURN_TYPE | Fixed | F010 - opaque impl Layer return type |
| 4 | [WARNING] PHASE_6_FIXED_FILE_HAS_DUPLICATE_FRONTMATTER | Fixed | F017 - consolidated to single F0 block |
| 5 | [SUGGESTION] AUDIT_KIND_REGEX_DEFINED_BUT_UNUSED | Fixed | F019 - shared in core |

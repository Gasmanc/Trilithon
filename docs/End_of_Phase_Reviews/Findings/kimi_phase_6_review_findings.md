# Kimi — Phase 6 Review Findings

**Reviewer:** cc-kimi
**Phase:** 6 — Audit log with secrets-aware redactor
**Date:** 2026-05-09

---

[HIGH] TLS_CORRELATION_ID_NOT_RESTORED_ON_PANIC
File: core/crates/adapters/src/tracing_correlation.rs
Lines: 175-190
Description: `CorrelationSpan::poll` sets the thread-local `CURRENT_CORRELATION_ID` before polling the inner future and restores it after the poll returns. If the inner future panics, the restore code never runs, leaving the stale correlation ID on the thread. The next task scheduled on that thread that reads `current_correlation_id()` outside of a span will see the leaked value.
Suggestion: Wrap the restoration in a drop guard (e.g., a struct that restores the previous value in its `Drop` impl) so the TLS is always reset even when the stack unwinds.

[HIGH] RFC6901_JSON_POINTER_DECODE_ORDER_INCORRECT
File: core/crates/core/src/schema/mod.rs
Lines: 96-103
Description: `decoded_segments` decodes JSON Pointer escape sequences with `seg.replace("~1", "/").replace("~0", "~")`. Naive sequential replacement produces wrong results for valid encoded segments such as `~1~0` (encoded `/~`), which decodes to `/~~` instead of `/~`. This breaks `SchemaRegistry::is_secret_field` for paths containing `~` or `/` in segments, potentially causing secret fields at those paths to leak into the audit log.
Suggestion: Replace the two `replace` calls with a single left-to-right scan that emits `~` on `~0`, `/` on `~1`, and leaves other characters unchanged.

[WARNING] PHASE_6_ARTEFACT_FILES_HAVE_MULTIPLE_FRONTMATTER_BLOCKS
File: docs/In_Flight_Reviews/Fixed/phase_6_fixed.md
Lines: general
Description: The file contains three consecutive YAML frontmatter blocks, violating the Foundation 0 schema rule of one finding per file. This will break `xtask audit-finding-schema` and other F0 tooling that expects a single `---\n...\n---` header.
Suggestion: Split each finding into its own file, or remove the spurious duplicate frontmatter blocks so only one remains per file.

[SUGGESTION] CADDY_INSTANCE_ID_HARDCODED_TO_LOCAL
File: core/crates/adapters/src/audit_writer.rs
Lines: 130
Description: `AuditWriter::record` hardcodes `caddy_instance_id: "local".to_owned()` in every audit row. In a multi-instance deployment there is no way to distinguish which instance produced an event.
Suggestion: Accept `caddy_instance_id` as a constructor parameter or read it from a config source so deployments can set a meaningful instance identifier.

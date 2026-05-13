# Phase 6 — Fixed Findings

**Run date:** 2026-05-13
**Total fixed:** 22 (pending gate verification)

| ID | Severity | Title | File | Commit | PR | Date |
|----|----------|-------|------|--------|----|------|
| F001 | CRITICAL | Migration 0006 fallback CREATE TABLE strips required columns | `core/crates/adapters/migrations/0006_audit_immutable.sql` | pending | — | 2026-05-13 |
| F007 | HIGH | actor_kind/outcome canonical JSON uses Debug repr | `core/crates/core/src/storage/{types.rs,helpers.rs}`, `core/crates/adapters/src/sqlite_storage.rs` | pending | — | 2026-05-13 |
| F010 | WARNING | correlation_layer opaque return type | `core/crates/adapters/src/tracing_correlation.rs` | pending | — | 2026-05-13 |
| F011 | WARNING | notes/target_id contract documented | `core/crates/adapters/src/audit_writer.rs` | pending | — | 2026-05-13 |
| F012 | WARNING | Fixed-depth secret-match assumption pinned by test | `core/crates/core/src/schema/mod.rs` | pending | — | 2026-05-13 |
| F013 | WARNING | Hash-prefix oracle documented on CiphertextHasher | `core/crates/core/src/audit/redactor.rs` | pending | — | 2026-05-13 |
| F014 | WARNING | X-Correlation-Id trust boundary documented | `core/crates/adapters/src/tracing_correlation.rs` | pending | — | 2026-05-13 |
| F015 | WARNING | verify_audit_chain added | `core/crates/core/src/storage/helpers.rs` | pending | — | 2026-05-13 |
| F016 | WARNING | Read-side kind validation removed (rollback-safe reads) | `core/crates/adapters/src/sqlite_storage.rs` | pending | — | 2026-05-13 |
| F017 | WARNING | phase_6_fixed.md frontmatter consolidated | `docs/In_Flight_Reviews/Fixed/phase_6_fixed.md` | pending | — | 2026-05-13 |
| F018 | WARNING | Phase 6 TODO migration name updated to 0006 | `docs/todo/phase-06-audit-log.md` | pending | — | 2026-05-13 |
| F019 | WARNING | Shared validate_audit_kind_pattern in core | `core/crates/core/src/audit/event.rs`, `core/crates/adapters/src/storage_sqlite/audit.rs` | pending | — | 2026-05-13 |
| F022 | WARNING | core/README.md audit pipeline section added | `core/README.md` | pending | — | 2026-05-13 |
| F023 | WARNING | Tier-1 secret fields expanded (TLS keys, bearer/token) | `core/crates/core/src/schema/secret_fields.rs` | pending | — | 2026-05-13 |
| F024 | WARNING | In-memory cursor pagination sorted by id DESC | `core/crates/core/src/storage/in_memory.rs` | pending | — | 2026-05-13 |
| F025 | WARNING | AUDIT_KIND_VOCAB cardinality assertion added | `core/crates/core/src/audit/event.rs` | pending | — | 2026-05-13 |
| F026 | WARNING | AuditAppend::from_current_span helper | `core/crates/adapters/src/audit_writer.rs` | pending | — | 2026-05-13 |
| F027 | SUGGESTION | caddy_instance_id constructor parameter | `core/crates/adapters/src/audit_writer.rs` | pending | — | 2026-05-13 |
| F028 | SUGGESTION | BEGIN IMMEDIATE isolation contract documented | `core/crates/adapters/src/sqlite_storage.rs` | pending | — | 2026-05-13 |
| F029 | SUGGESTION | MAX_REDACTOR_DEPTH guard against deep JSON | `core/crates/core/src/audit/redactor.rs` | pending | — | 2026-05-13 |
| F030 | SUGGESTION | notes/target_id length caps + FieldTooLong error | `core/crates/adapters/src/audit_writer.rs` | pending | — | 2026-05-13 |
| F032 | WARNING | Directory-based bypass guard allowlist | `core/crates/adapters/tests/audit_writer_no_bypass.rs` | pending | — | 2026-05-13 |

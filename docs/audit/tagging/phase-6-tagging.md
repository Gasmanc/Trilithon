# Phase 6 — Tagging Analysis
**Generated:** 2026-05-08
**Model:** opus (extended thinking)
**Documents read:**
- /Users/carter/Coding/Trilithon/CLAUDE.md (192 lines)
- /Users/carter/Coding/Trilithon/docs/architecture/architecture.md (1124 lines)
- /Users/carter/Coding/Trilithon/docs/architecture/trait-signatures.md (734 lines)
- /Users/carter/Coding/Trilithon/docs/planning/PRD.md (952 lines)
- /Users/carter/Coding/Trilithon/docs/phases/phase-06-audit-log.md (87 lines)
- /Users/carter/Coding/Trilithon/docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md (185 lines)
- /Users/carter/Coding/Trilithon/docs/adr/0014-secrets-encrypted-at-rest-with-keychain-master-key.md (209 lines)
- /Users/carter/Coding/Trilithon/docs/todo/phase-06-audit-log.md (829 lines)
**Slices analysed:** 7

Note: This project does not contain `docs/architecture/seams.md` or `docs/architecture/contract-roots.toml`. "Affected seams" is reported as `none` for every slice and "Planned contract additions" is best-effort based on `pub` items declared in the slice docs.

---

## Proposed Tags

### 6.1: `AuditEvent` enum and Display-to-wire mapping in `core`
**Proposed tag:** [cross-cutting]
**Reasoning:** Although all three files live in the `core` crate with no I/O, this slice establishes the closed Tier 1 audit-event vocabulary plus the `AUDIT_KIND_REGEX` convention that every subsequent slice (6.4 storage validation, 6.5 writer, 6.6 query parser via `AuditEvent::from_str`) and every future audit-emitting phase (7, 8, 9, 10) must conform to. The `kind_str()` ↔ architecture §6.6 wire-string mapping is the canonical naming convention other slices follow, and ADR-0009 plus PRD T1.7/T1.15 are referenced. That triggers the "introduces a tracing/audit/logging convention other slices must follow" rubric clause.
**Affected seams:** none
**Planned contract additions:** `core::audit::AuditEvent`, `core::audit::AuditEventParseError`, `core::audit::AuditEvent::kind_str`, `core::audit::AUDIT_KIND_REGEX`
**Confidence:** high

### 6.2: `AuditEventRow`, `AuditSelector`, `AuditOutcome`, `ActorRef`
**Proposed tag:** [standard]
**Reasoning:** Files live entirely inside the `core` crate (`audit/row.rs`, `audit/mod.rs`); no traits introduced, no I/O, no migration. The new types are wire records consumed by the existing `Storage::record_audit_event` and `Storage::tail_audit_log` trait methods (declared in trait-signatures.md §1, landed in Phase 2) — this slice does not modify that trait. Although several adapter slices depend on these types, they are passive data carriers rather than a convention. Larger and more API-shaped than 6.1, but still self-contained within one crate and one module.
**Affected seams:** none
**Planned contract additions:** `core::audit::AuditEventRow`, `core::audit::AuditSelector`, `core::audit::AuditOutcome`, `core::audit::ActorRef`, `core::audit::AuditRowId`, `core::audit::AUDIT_QUERY_DEFAULT_LIMIT`, `core::audit::AUDIT_QUERY_MAX_LIMIT`
**Confidence:** high

### 6.3: `SecretsRedactor` over `serde_json::Value` plus diff redaction
**Proposed tag:** [cross-cutting]
**Reasoning:** All files reside in `core`, but the slice introduces a new pure-core trait (`CiphertextHasher`) plus the `SecretsRedactor` that becomes the project-wide gate for hazard H10 — every diff every audit writer ever produces must pass through it (Phase 6.5, plus all future audit-emitting phases). It cites ADR-0009, ADR-0014, PRD T1.7/T1.15, hazard H10, and architecture §6.6 (3+ refs). The `REDACTION_PREFIX`/`HASH_PREFIX_LEN` invariants and the schema secret-field registry are conventions all later phases must respect, and trait-signatures.md §3 notes the surface is designed to be wired to `SecretsVault::redact` in Phase 10. Multiple rubric triggers fire: new trait surface, project-wide convention, 3+ ADR/PRD references.
**Affected seams:** none
**Planned contract additions:** `core::audit::redactor::SecretsRedactor`, `core::audit::redactor::CiphertextHasher`, `core::audit::redactor::RedactionResult`, `core::audit::redactor::RedactorError`, `core::audit::redactor::REDACTION_PREFIX`, `core::audit::redactor::HASH_PREFIX_LEN`, `core::schema::secret_fields::TIER_1_SECRET_FIELDS`
**Confidence:** high

### 6.4: Migration `0003_audit_immutable.sql` plus storage-side kind validation
**Proposed tag:** [cross-cutting]
**Reasoning:** This is the textbook cross-cutting trigger from the rubric: "schema migration with triggers" and "migration that other slices structurally depend on." Files span the `adapters` crate (DDL + adapter Rust), the migration consumes types from `core` (`AuditEvent`, `AuditEventRow`, `AuditRowId`, `StorageError::AuditKindUnknown`), and adds `BEFORE UPDATE`/`BEFORE DELETE` immutability triggers per ADR-0009 plus emits the `storage.migrations.applied` tracing event from architecture §12.1. Slices 6.5, 6.6, 6.7, and the entirety of audit emission across phases 7–10 structurally depend on this table existing with these triggers.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::storage::StorageError::AuditKindUnknown` variant (referenced); migration file itself is not a Rust contract.
**Confidence:** high

### 6.5: `AuditWriter::record` adapter wired to `Storage::record_audit_event`
**Proposed tag:** [cross-cutting]
**Reasoning:** Files live in the `adapters` crate, but the slice depends on three core surfaces (`AuditEvent`, `SecretsRedactor`, `Storage`, `Clock`) and explicitly establishes the project-wide invariant "the single, public path into `audit_log`" — enforced by a no-bypass test that constrains every other code site in the workspace. Any future slice that emits an audit row must call this writer; that is the "introduces a logging convention other slices must follow" trigger. Cites ADR-0009, PRD T1.7, T1.15, architecture §6.6 and §7.1 (3+ references). Bridges core types to the adapter Storage trait — a deliberate layer-coupling surface.
**Affected seams:** none
**Planned contract additions:** `trilithon_adapters::AuditWriter`, `trilithon_adapters::AuditAppend`, `trilithon_adapters::AuditWriteError`
**Confidence:** high

### 6.6: Audit query API with paginated filters
**Proposed tag:** [standard]
**Reasoning:** All files are inside `adapters/src/storage_sqlite/` (one module of one crate). This slice implements one already-declared trait method (`Storage::tail_audit_log`) — it does not introduce or modify a shared trait. I/O is local to the existing SQLite adapter. Consumes core types (`AuditSelector`, `AuditEventRow`) but adds no convention. References PRD T1.7, architecture §6.6, trait-signatures.md §1 (under three ADR/PRD refs). Phase 9 will surface this via HTTP, but that's downstream consumption, not a structural dependency on a new shape introduced here. Fits the "query API in one module" standard example verbatim.
**Affected seams:** none
**Planned contract additions:** none (implements an existing trait method on `SqliteStorage`)
**Confidence:** high

### 6.7: Tracing layer that injects and propagates `correlation_id`
**Proposed tag:** [cross-cutting]
**Reasoning:** Files explicitly span three layers/crates (`adapters/src/tracing_correlation.rs`, `adapters/src/lib.rs`, `cli/src/main.rs`) — crossing the adapters↔cli layer boundary at `tracing-subscriber` registration. The slice ships `CORRELATION_ID_FIELD`, `current_correlation_id()`, and `with_correlation_span()` as the project-wide convention every audit-emitting code path (HTTP, schedulers, signal handlers, background loops, the writer from 6.5) must use to populate `audit_log.correlation_id`. The slice doc itself states "this is the convention all other slices must follow." It also flags adding `correlation_id.missing` to architecture §12.1 — i.e. amends the tracing vocabulary. Multiple rubric triggers fire: cross-crate, layer-crossing, project-wide tracing convention.
**Affected seams:** none
**Planned contract additions:** `trilithon_adapters::tracing_correlation::CORRELATION_ID_FIELD`, `current_correlation_id`, `with_correlation_span`, `correlation_layer`
**Confidence:** high

---

## Summary
- 2 trivial: 0
- standard: 2 (6.2, 6.6)
- cross-cutting: 5 (6.1, 6.3, 6.4, 6.5, 6.7)
- low-confidence: 0

(Counts: 0 trivial, 2 standard, 5 cross-cutting, 0 low-confidence.)

## Notes

- Phase 6 is unusually convention-heavy: it lays the audit substrate that every subsequent phase emits into. Five of seven slices set rules other slices must follow (the vocabulary 6.1, the redactor + secret-field registry 6.3, the immutable storage + kind validation 6.4, the single-writer invariant 6.5, the correlation-id propagation rule 6.7). Only 6.2 (passive data types) and 6.6 (a localised SQL implementation of an already-declared trait method) are self-contained.
- Dependency order in the slice plan summary is correct and matches the tag distribution: every cross-cutting slice precedes the standard slices that consume it (6.1 → 6.2; 6.4 → 6.6; 6.5 → 6.7).
- 6.5's "no bypass" enforcement test is itself a cross-cutting constraint on the workspace: future slices must route audit emission through `AuditWriter::record`, not directly through `Storage::record_audit_event`. /phase implementers should be primed for this when reviewing later phases.
- Architecture §12.1 amendment (`correlation_id.missing`) flagged by 6.7 should land in the same commit as the slice per CLAUDE.md "no inventing names silently" rule.
- This project lacks `docs/architecture/seams.md` and `docs/architecture/contract-roots.toml`; "Affected seams" is `none` everywhere by directive. Contract additions are best-effort listings of `pub` items declared in the slice docs.

---

## User Decision
**Date:** 2026-05-08
**Decision:** accepted

### Modifications (if any)
None.

### Notes from user
None.

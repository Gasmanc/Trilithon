# Scope Guardian — Phase 6 Review Findings

Phase: `phase-06` — Audit log with secrets-aware redactor
Diff range: `5e0b13f..HEAD`
Reviewer: scope-guardian
Date: 2026-05-09

---

[HIGH] DUPLICATE AuditEventRow — NEW TYPE NOT WIRED INTO ADAPTERS
File: core/crates/core/src/audit/row.rs / core/crates/core/src/storage/types.rs
Lines: row.rs:85, types.rs:136
Description: Slice 6.2 specifies "AuditEventRow, AuditSelector, AuditOutcome, ActorRef" as the canonical wire types exchanged between core and the Storage trait. The diff adds exactly these types in `core::audit::row`. However, all production adapter code — `audit_writer.rs`, `sqlite_storage.rs`, and `storage_sqlite/audit.rs` — continues to import `trilithon_core::storage::types::AuditEventRow`, a pre-existing type with a diverging schema that includes `prev_hash: String`, `caddy_instance_id: String`, `actor_kind: ActorKind`, and `actor_id: String` (flat strings) rather than the spec-mandated `actor: ActorRef` enum. The `core::audit::row::AuditEventRow` (which does match the spec) is exported but unused in any production path. `audit_writer.rs` explicitly constructs `storage::types::AuditEventRow` at line 182. The `core::audit::row` types are effectively dead code at the adapter boundary. The spec says these types ARE the storage trait wire surface — they are not.
TODO unit: 6.2 — AuditEventRow, AuditSelector, AuditOutcome, ActorRef
Suggestion: Wire `core::audit::row::AuditEventRow` into `Storage::record_audit_event` and `Storage::tail_audit_log` trait signatures, migrate `audit_writer.rs` and `sqlite_storage.rs` to use `core::audit::row::AuditEventRow` as the single canonical type, and retire the duplicate definition in `storage::types`. The `prev_hash` and `caddy_instance_id` fields belong to the chain-hash machinery that pre-dates Phase 6; if they must survive, they should be clearly separated from the Phase 6 spec row rather than silently retained in the active type.

---

[HIGH] AUDITWRITER ActorRef REDEFINED IN ADAPTERS — SHOULD USE CORE TYPE
File: core/crates/adapters/src/audit_writer.rs
Lines: 47-84
Description: Slice 6.2 defines `ActorRef` as a pure-core type in `core::audit::row`. Slice 6.5 specifies that `AuditWriter` takes the core types. However, `audit_writer.rs` declares its own `ActorRef` enum at the adapter boundary ("Mirrors `audit::row::ActorRef` but lives at the adapter boundary so that callers outside `core` can construct it without a `core`-internal import"). This creates two `ActorRef` definitions with independent maintenance, and makes the comment in the file acknowledge the duplication explicitly. Because `core::audit::row::ActorRef` is already a public type in `core`, a separate re-declaration at the adapter layer is pure duplication: any caller that can already use `AuditAppend` can import from `core`.
TODO unit: 6.5 — AuditWriter::record adapter wired to Storage::record_audit_event
Suggestion: Delete the `ActorRef` redefinition from `audit_writer.rs` and import `core::audit::row::ActorRef` directly. If callers need a re-export, add `pub use trilithon_core::audit::ActorRef;` to `adapters::lib`.

---

[WARNING] MIGRATION FILE NAMED 0006 BUT TODO SPECIFIES 0003
File: core/crates/adapters/migrations/0006_audit_immutable.sql
Lines: general
Description: Slice 6.4 specifies the migration file as `0003_audit_immutable.sql` and refers to "schema version 2" upgrading to version 3. The delivered file is `0006_audit_immutable.sql`. This is likely a pragmatic numbering adjustment to avoid collisions with migrations 0003–0005 that were already in place, but the deviation from the spec-mandated name is not documented in the diff, and the TODO's cross-references (architecture §14, `schema_migrations` version numbering) now point to the wrong number. The SQL content itself matches the spec exactly.
TODO unit: 6.4 — Migration 0003_audit_immutable.sql plus storage-side kind validation
Suggestion: Update the TODO and architecture cross-references to reflect `0006_audit_immutable.sql` and version 6, or add a comment in the migration file explaining the renaming decision, so there is no silent drift between spec and implementation.

---

[WARNING] AUDIT_KIND_REGEX CONSTANT NOT USED — PATTERN DUPLICATED INLINE
File: core/crates/core/src/audit/event.rs, core/crates/adapters/src/storage_sqlite/audit.rs
Lines: event.rs:265, storage_sqlite/audit.rs:15-30
Description: Slice 6.1 declares `AUDIT_KIND_REGEX: &str` as a compile-time constant, and slice 6.4 specifies that `validate_kind` uses it. In the diff, `AUDIT_KIND_REGEX` is defined in `event.rs` but is never imported by `storage_sqlite/audit.rs`. Instead, `validate_kind_pattern` in `storage_sqlite/audit.rs` re-implements the identical segment-matching logic inline (with a comment "Manual match — avoids a `regex` dependency in adapters"). The inlined logic is semantically equivalent but duplicates the pattern. The spec contract ("kind MUST match AUDIT_KIND_REGEX") is technically met via equivalent logic, but `AUDIT_KIND_REGEX` itself is only used in unit tests inside `event.rs` — it is dead in production paths.
TODO unit: 6.4 — Migration 0003_audit_immutable.sql plus storage-side kind validation
Suggestion: Either import `AUDIT_KIND_REGEX` from `event.rs` in `storage_sqlite/audit.rs` and reference it in a doc comment, or make `AuditEvent::validate_kind_str` a `core` function so the storage adapter calls it rather than reimplementing the check.

---

[WARNING] AuditEvent ENUM CONTAINS VARIANTS BEYOND TIER 1 SPEC SET
File: core/crates/core/src/audit/event.rs
Lines: 86-125
Description: Slice 6.1 specifies a closed Tier 1 set of 22 variants across 5 groups (Auth, Caddy, Config, Mutation, Secrets). The delivered enum contains 44 variants, adding: `AuthBootstrapCredentialsCreated`, `DriftDeferred`, `DriftAutoDeferred`, `ConfigRebased`, `MutationRebasedAuto`, `MutationRebasedManual`, `MutationRebaseExpired`, plus entirely new groups — Policy Presets (4 variants), Import/Export (4 variants), Tool Gateway (3 variants), Docker (1 variant), and Proposals (3 variants). No work unit in slices 6.1–6.7 calls for these additions. The `#[non_exhaustive]` attribute exists precisely so later phases can extend the vocabulary without a Phase 6 expansion. The prompt instructs that the user has flagged compound-engineering onboarding commits as intentional; it does not extend that exemption to vocabulary growth.
TODO unit: 6.1 — AuditEvent enum and Display-to-wire mapping in core
Suggestion: Revert to the 22 Tier 1 variants specified by slice 6.1. Defer the additional vocabulary to the phases that will actually emit those events (policy management, import/export, tool gateway, etc.), so the vocabulary and the code that produces each event land together and are reviewable as a unit.

---

[WARNING] SLICE 6.7 EXIT CONDITION UNMET — correlation_layer NOT REGISTERED IN CLI
File: core/crates/cli/src/main.rs
Lines: general
Description: Slice 6.7 specifies: "core/crates/cli/src/main.rs — register the layer at subscriber init." The `tracing_correlation.rs` module ships correctly, but `cli/src/main.rs` contains zero references to `correlation_layer`, `with_correlation_span`, or `tracing_correlation`. The `correlation_layer()` function returns `tower::layer::util::Identity` and is documented as "Phase 9 attaches it", which is consistent with the spec note that Phase 9 wires the HTTP middleware. However, the spec also says the CLI registers the layer at subscriber init for background-task propagation, and the tests for `background_task_seeds_per_iteration` require that background tasks run inside correlation spans. The background-task seeding path is tested in isolation but is not wired into the daemon entry point.
TODO unit: 6.7 — Tracing layer that injects and propagates correlation_id
Suggestion: Add `with_correlation_span(Ulid::new(), "system", "daemon", ...)` wrappers around background task entry points in `cli/src/run.rs` or equivalent, consistent with the slice 6.7 algorithm step 4: "Background loops call `with_correlation_span(Ulid::new(), "system", component_name, fut)` once per iteration."

---

[WARNING] PHASE EXIT CHECKLIST — core/README.md NOT UPDATED
File: core/README.md
Lines: general
Description: The phase exit checklist explicitly requires: "core/README.md records the audit pipeline and the redactor invariant, citing ADR-0009." The diff contains no change to `core/README.md`. The file currently mentions audit only in passing (sentinel takeover and snapshot immutability). The audit pipeline, `AuditWriter` single-path invariant, and `SecretsRedactor` guarantee are unrecorded in any operator-visible documentation.
TODO unit: Phase exit checklist
Suggestion: Add a section to `core/README.md` describing the audit pipeline (`AuditWriter` is the only write path, `SecretsRedactor` gates every diff, triggers enforce immutability), citing ADR-0009.

---

Scope verdict: mixed
Coherence verdict: partially coherent

Notes:
- The compound engineering onboarding commits (xtask crate, seams.md, contract-roots.toml, zero-debt.yaml cross_phase block, web/.prettierrc, review finding frontmatter migrations) are confirmed as intentional per the prompt and are not flagged as scope creep.
- All 7 slices have corresponding code changes; no slice is entirely absent.
- The HIGH findings concern the `AuditEventRow` type split — the spec types exist but are not wired into the production path the spec designates them for. The adapter layer continues operating on the pre-Phase-6 `storage::types::AuditEventRow` rather than the newly delivered `audit::row::AuditEventRow`.

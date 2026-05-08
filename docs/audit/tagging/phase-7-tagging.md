# Phase 7 — Tagging Analysis
**Generated:** 2026-05-09
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md (208 lines)
- docs/architecture/architecture.md (1124 lines)
- docs/architecture/trait-signatures.md (734 lines)
- docs/planning/PRD.md (952 lines)
- docs/phases/phase-07-apply-path.md (94 lines)
- docs/adr/0002, 0009, 0012, 0013 (and 0015 referenced by 7.1)
- docs/architecture/seams.md (98 lines, empty registry)
- docs/architecture/contract-roots.toml (41 lines, empty roots)
- docs/todo/phase-07-apply-path.md (758 lines)
**Slices analysed:** 8

---

## Proposed Tags

### 7.1: CaddyJsonRenderer deterministic serialisation in pure core
**Proposed tag:** [standard]
**Reasoning:** All file changes are confined to the core crate (`core/crates/core/src/reconciler/{mod.rs,render.rs}` and `lib.rs`). It introduces a new pub trait `CaddyJsonRenderer` plus `DefaultCaddyJsonRenderer`, `RenderError`, and `canonical_json_bytes` — but consumers of this trait live in slice 7.4 (adapters), so the trait surface itself is local-to-core at this slice. No I/O, no async, no cross-layer dep added. References ADR-0002, ADR-0015, plus PRD T1.1/T1.6 — within the 3-ADR threshold. The renderer is structurally depended on by Phases 4/5/6 for content addressing, but only via the canonical-bytes guarantee, not via a shared mutable trait.
**Affected seams:** none (registry is empty; renderer ↔ applier seam emerges in 7.4)
**Planned contract additions:** `trilithon_core::reconciler::CaddyJsonRenderer`, `trilithon_core::reconciler::DefaultCaddyJsonRenderer`, `trilithon_core::reconciler::render::RenderError`, `trilithon_core::reconciler::render::canonical_json_bytes`
**Confidence:** high
**If low confidence, why:** —

### 7.2: ApplyOutcome, ApplyError, and apply-state types
**Proposed tag:** [standard]
**Reasoning:** Single file `core/crates/core/src/reconciler/applier.rs`, types-only, no I/O, no trait implementation. However it is not pure-trivial: it introduces five new pub enums (`ApplyOutcome`, `AppliedState`, `ReloadKind`, `ApplyFailureKind`, `ApplyError`) consumed by the HTTP layer in Phase 9 and the adapter in slice 7.4 — these become part of the public contract surface for the reconciler boundary. References PRD T1.1, trait-signatures.md §6, architecture §7.1.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::reconciler::ApplyOutcome`, `trilithon_core::reconciler::AppliedState`, `trilithon_core::reconciler::ReloadKind`, `trilithon_core::reconciler::ApplyFailureKind`, `trilithon_core::reconciler::ApplyError`
**Confidence:** high
**If low confidence, why:** —

### 7.3: Capability re-check at apply time
**Proposed tag:** [standard]
**Reasoning:** Single new file `core/crates/core/src/reconciler/capability_check.rs`, pure function, no I/O. Consumes existing `core::caddy::CapabilitySet` (no trait modified). The re-check is local logic; the applier slice composes it. References ADR-0013, PRD T1.1/T1.11, hazard H5, architecture §7.4 — within thresholds.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::reconciler::capability_check::CapabilityCheckError`, `trilithon_core::reconciler::capability_check::check_against_capability_set`
**Confidence:** high
**If low confidence, why:** —

### 7.4: Applier adapter — happy path with audit ApplyStarted/ApplySucceeded
**Proposed tag:** [cross-cutting]
**Reasoning:** First implementation of the `core::reconciler::Applier` trait in adapters; wires together CaddyClient, DiffEngine, Storage, AuditWriter, CapabilityCache, Renderer, and Clock — at least seven cross-module dependencies pulled into one adapter. Files span both crates structurally (adapters consuming many core traits). Introduces three tracing event names (`apply.started`, `apply.succeeded`, `apply.failed`) and three audit kind emissions (`config.applied`, `config.apply-failed`, `caddy.unreachable`) that subsequent slices (7.5/7.6/7.7/7.8) and Phase 9 structurally depend on. References ADR-0002, ADR-0009, ADR-0013 plus multiple PRD items (T1.1, T1.6, T1.7) — clears the 3-ADR threshold. Architecture §7.1's eleven-step apply procedure is realised here.
**Affected seams:** PROPOSED: `applier-caddy-admin` (Applier ↔ CaddyClient — boundary between core reconciler trait and adapter HTTP client), PROPOSED: `applier-audit-writer` (Applier ↔ AuditWriter — terminal-row contract consumed by query path)
**Planned contract additions:** `trilithon_adapters::applier_caddy::CaddyApplier` (and impl `Applier` for it)
**Confidence:** high
**If low confidence, why:** —

### 7.5: Optimistic concurrency on config_version
**Proposed tag:** [cross-cutting]
**Reasoning:** Extends `applier_caddy.rs` AND adds new helpers in `storage_sqlite/snapshots.rs` — two distinct adapter modules touched. Adds a new `StorageError::OptimisticConflict` variant that propagates into core via `ApplyError::OptimisticConflict` — a new error-kind convention crossing the storage/reconciler boundary. References ADR-0012, hazard H8, architecture §6.5/§9. Introduces the `mutation.conflicted` audit-kind emission convention used by Phase 9's HTTP path UX. The CAS pattern here is structurally depended on by Phase 9 (T2.10).
**Affected seams:** PROPOSED: `snapshots-config-version-cas` (storage CAS ↔ applier — config_version pointer-advance contract)
**Planned contract additions:** `trilithon_adapters::storage_sqlite::snapshots::current_config_version`, `trilithon_adapters::storage_sqlite::snapshots::advance_config_version_if_eq`, new variant `trilithon_adapters::storage_sqlite::StorageError::OptimisticConflict`
**Confidence:** high
**If low confidence, why:** —

### 7.6: In-process mutex plus SQLite advisory lock per caddy_instance_id
**Proposed tag:** [cross-cutting]
**Reasoning:** Touches `applier_caddy.rs`, adds new module `storage_sqlite/locks.rs`, AND ships a SQL migration `0004_apply_locks.sql`. Database migrations are structural shared state across phases; adding a new table is not a local change. Introduces a new `LockError` type and an `AcquiredLock` RAII guard whose `Drop` semantics are correctness-critical. References architecture §9 concurrency model and ADR-0012. The migration plus the per-instance mutex is depended on by 7.7's "exactly one terminal row" property and by any future apply-path consumer.
**Affected seams:** PROPOSED: `apply-lock-coordination` (in-process mutex + SQLite advisory lock — mutual-exclusion contract per caddy_instance_id)
**Planned contract additions:** `trilithon_adapters::storage_sqlite::locks::LockError`, `trilithon_adapters::storage_sqlite::locks::AcquiredLock`, `trilithon_adapters::storage_sqlite::locks::acquire_apply_lock`; migration file `0004_apply_locks.sql`
**Confidence:** medium
**If low confidence, why:** Could be argued [standard] since it stays inside the adapters crate, but the schema migration plus RAII-guard semantics that other slices rely on push it to cross-cutting.

### 7.7: Failure handling and reload-semantics audit metadata
**Proposed tag:** [cross-cutting]
**Reasoning:** Modifies `applier_caddy.rs` AND extends core's `applier.rs` (`ReloadKind` may gain a `drain_window_ms` field). Crosses the core/adapters boundary. Introduces a new pub serde-stable struct `ApplyAuditNotes` plus `AppliedStateTag` that becomes the audit-notes wire format consumed by query/UI paths (Phase 13/17) — a logging/audit convention other slices and phases must follow. Hazard H4 and PRD T1.7 explicitly tracked. The "exactly one terminal" property is a cross-slice invariant verified here.
**Affected seams:** PROPOSED: `apply-audit-notes-format` (ApplyAuditNotes serde wire format ↔ audit-log query consumers)
**Planned contract additions:** `trilithon_core::reconciler::applier::ApplyAuditNotes`, `trilithon_core::reconciler::applier::AppliedStateTag`, possible shape change to `trilithon_core::reconciler::ReloadKind`
**Confidence:** high
**If low confidence, why:** —

### 7.8: TLS-issuance state separation in audit metadata
**Proposed tag:** [standard]
**Reasoning:** Adds new module `tls_observer.rs` and modifies `applier_caddy.rs` — both inside the adapters crate. Implements one logical bounded-task abstraction; reuses the audit-notes format established in 7.7 (no new audit-notes convention). Spawns a background task that emits already-defined audit kinds (`config.applied`, `config.apply-failed`). Hazard H17 and PRD T1.1 referenced; only one ADR/architecture section. No core types introduced. The follow-up emission semantics are local to the observer.
**Affected seams:** none (reuses 7.4/7.7 seams)
**Planned contract additions:** `trilithon_adapters::tls_observer::TlsIssuanceObserver`
**Confidence:** medium
**If low confidence, why:** Spawning a background tokio task that emits audit rows after `apply()` returns is a new lifecycle pattern — could be argued cross-cutting if Phase 13's audit query path needs to reason about correlated follow-up rows. Kept [standard] because the new pattern is contained to one adapter file and reuses the existing audit-notes format.

---

## Summary
- 0 trivial
- 4 standard (7.1, 7.2, 7.3, 7.8)
- 4 cross-cutting (7.4, 7.5, 7.6, 7.7)
- 0 low-confidence (require human review)

## Notes
- The Phase 7 slice list is unusually concentrated at the adapters↔core boundary: every slice from 7.4 onward extends the same `applier_caddy.rs` file with progressively more cross-cutting concerns (locking, CAS pointer-advance, audit-notes wire format, TLS follow-up). This is by design — the apply path is a single architectural seam that the slice plan is layering capabilities onto.
- No slice is purely [trivial]: even the type-only slices (7.2) introduce contract surface consumed by Phase 9's HTTP layer, so they are [standard] not [trivial] under this rubric.
- Both the seam registry (`seams.md`) and the contract-roots (`contract-roots.toml`) are empty in this project; all "Affected seams" entries are PROPOSED additions to `seams-proposed.md` for `/phase-merge-review` to ratify.
- Slice 7.7 may force a backward-incompatible shape change on `ReloadKind` introduced in 7.2. If so, 7.2 should land the final shape (variant with `drain_window_ms`) up front to avoid a contract-version churn within the same phase.
- The phase reference's "Open question" about an `ApplyStarted` audit kind is intentionally deferred — slice 7.4's algorithm step 2 commits to NOT emitting it. Reviewers should confirm this is locked in before tagging finalises.

---

## User Decision
**Date:** 2026-05-09
**Decision:** accepted

### Modifications (if any)
None.

### Notes from user
None.

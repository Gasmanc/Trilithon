# Phase 02 — Tagging Analysis
**Generated:** 2026-05-02
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md (184 lines)
- docs/architecture/architecture.md (1124 lines)
- docs/architecture/trait-signatures.md (734 lines)
- docs/planning/PRD.md (952 lines)
- docs/phases/phase-02-sqlite-persistence.md (104 lines)
- docs/adr/0006-sqlite-as-v1-persistence-layer.md (162 lines)
- docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md (185 lines)
- docs/adr/0012-optimistic-concurrency-on-monotonic-config-version.md (186 lines)
- docs/adr/0003-rust-three-layer-workspace-architecture.md (160 lines)
- docs/todo/phase-02-sqlite-persistence.md (931 lines)

**Slices analysed:** 7

---

## Proposed Tags

### 2.1: `Storage` trait surface and `StorageError`
**Proposed tag:** cross-cutting
**Reasoning:** Introduces the `Storage` trait — the canonical persistence interface that slices 2.2, 2.4, and downstream phases (snapshot writer, audit log, mutation queue) all implement. New shared trait surface in `core` that is the load-bearing interface for the entire persistence layer. The slice references trait-signatures §1, architecture §6, ADR-0006, ADR-0009, and ADR-0012 (via row shapes / config_version), and defines `StorageError` as the shared error vocabulary every other slice and phase consumes.
**Confidence:** high

### 2.2: `InMemoryStorage` test double
**Proposed tag:** cross-cutting
**Reasoning:** Implements the `Storage` trait (a shared trait), and — critically — introduces `audit_vocab.rs` as the single source of truth for the §6.6 audit `kind` vocabulary. Slice 2.4's `SqliteStorage` imports the same vocab list, so this slice establishes a convention that other slices structurally depend on. The double is test-only but the vocab module it adds is production code that crosses into 2.4 and beyond.
**Confidence:** medium
**If low confidence, why:** Borderline standard if `audit_vocab.rs` were trivially small; the cross-slice consumption of the constant by 2.4 tips it to cross-cutting.

### 2.3: `0001_init.sql` migration with seven Tier 1 tables
**Proposed tag:** cross-cutting
**Reasoning:** A schema migration that other slices structurally depend on (2.4 queries these tables, 2.5 runs the migrator over it, 2.7 wires startup). Creates 9 tables that every subsequent phase's writers/readers reference. References architecture §6.1–§6.10, ADR-0006, ADR-0009; establishes the `caddy_instance_id DEFAULT 'local'` convention that all future migrations must follow. Substrate migration explicitly listed as a cross-cutting example in the rubric.
**Confidence:** high

### 2.4: `SqliteStorage` adapter, pragmas, advisory lock
**Proposed tag:** cross-cutting
**Reasoning:** Files span `core` (imports `Storage` trait + `audit_vocab`) and `adapters` (new module + integration tests), explicitly crossing the core↔adapters boundary. Implements the shared `Storage` trait, introduces the advisory-lock convention (`trilithon.lock`) consumed by 2.7, and references trait-signatures §1, architecture §6 + §10 (H14), and ADR-0006. The pragmas + lock + adapter wiring are the load-bearing production implementation that 2.5/2.6/2.7 all build on.
**Confidence:** high

### 2.5: Migration runner with downgrade refusal
**Proposed tag:** standard
**Reasoning:** Single crate (`adapters`), one new module + one re-export, no new shared trait. Introduces `MigrationError` and the `storage.migrations.applied` tracing event, but the event is consumed only by 2.7's startup wiring (already in scope for that slice) — not a convention other slices must follow. No cross-layer dependency added. Sits cleanly inside the adapter layer.
**Confidence:** medium
**If low confidence, why:** The `storage.migrations.applied` tracing event is a documented architecture §12.1 event consumed across the daemon, which arguably edges toward cross-cutting; however, only one downstream slice (2.7) reads it and the event lives entirely in `adapters`.

### 2.6: Periodic `PRAGMA integrity_check` task
**Proposed tag:** cross-cutting
**Reasoning:** The slice itself documents that it must RELOCATE `ShutdownSignal` from `cli` into `core` (Open question 3) — explicitly crossing the cli↔core↔adapters layer boundary and modifying a shared lifecycle primitive that Phase 1's slice 1.5 owns. That relocation may also require an ADR-level decision about `tokio` in `core` (architecture §5 forbids it). Introduces the `storage.integrity-check.failed` tracing event (architecture §12.1) tied to H14. References architecture §10, §12.1, and pulls in lifecycle plumbing across three crates.
**Confidence:** high

### 2.7: Wire startup, exit code 3 on storage failure, integration tests
**Proposed tag:** cross-cutting
**Reasoning:** Files span `cli` (main.rs, exit.rs, integration tests) and consume APIs from `adapters` (SqliteStorage, apply_migrations, run_integrity_loop) and `core` (StorageError, MigrationError types via From impls). Crosses cli↔adapters↔core. Adds `ExitCode` mappings for `StorageError` and `MigrationError` — extending a shared exit-code convention. Wires the `daemon.started` ordering invariant (only after migrations succeed). References architecture §6, §10, §12.1, ADR-0006, ADR-0009.
**Confidence:** high

---

## Summary
- 0 trivial
- 1 standard
- 6 cross-cutting
- 2 low-confidence (require human review): 2.2, 2.5

## Notes

This phase is unusually cross-cutting because it lays the entire persistence substrate: a new shared trait (2.1), a substrate migration (2.3), the production adapter implementing the trait across the layer boundary (2.4), a lifecycle relocation (2.6), and end-to-end startup wiring (2.7). The dependency chain is strictly linear (2.1 → 2.2/2.3 → 2.4 → {2.5, 2.6} → 2.7), and every slice except 2.5 either defines or consumes a contract that other slices/phases depend on.

Slice 2.6 stands out: the slice doc itself flags an architecture-level open question (Open question 3) about whether `core` may take a `tokio` dependency or whether a `ShutdownObserver` trait is the right abstraction. Whichever path is chosen, the work touches three crates and modifies a Phase-1 lifecycle type — strongly cross-cutting and warrants the deeper reasoning budget.

Slice 2.2 is the closest call: the test double itself is contained, but the `audit_vocab` const it introduces is production code in `core` that 2.4 imports. If the implementer were to inline the vocab list in both places (rejected by the slice's "lives in exactly one file" exit condition), it would be standard; under the actual exit condition it is cross-cutting.

Slice 2.5 is the only non-cross-cutting slice. It is self-contained within `adapters`, implements no new trait, and adds one local error type plus one tracing event. Tagged standard with medium confidence because the tracing event is in the architecture's documented event catalog (§12.1), but no other slice in this phase emits or consumes it beyond 2.7's already-in-scope startup logging.

---

## User Decision
**Date:** 2026-05-02
**Decision:** accepted

### Modifications
None.

### Notes from user
None.

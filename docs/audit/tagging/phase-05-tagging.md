# Phase 05 — Tagging Analysis
**Generated:** 2026-05-05
**Model:** opus (extended thinking)
**Documents read:**
- /Users/carter/Coding/Trilithon/CLAUDE.md (184 lines)
- /Users/carter/Coding/Trilithon/docs/architecture/architecture.md (1124 lines)
- /Users/carter/Coding/Trilithon/docs/architecture/trait-signatures.md (734 lines)
- /Users/carter/Coding/Trilithon/docs/planning/PRD.md (952 lines)
- /Users/carter/Coding/Trilithon/docs/phases/phase-05-snapshot-writer.md (88 lines)
- /Users/carter/Coding/Trilithon/docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md (185 lines)
- /Users/carter/Coding/Trilithon/docs/adr/0012-optimistic-concurrency-on-monotonic-config-version.md (186 lines)
- /Users/carter/Coding/Trilithon/docs/adr/0003-rust-three-layer-workspace-architecture.md (160 lines)
- /Users/carter/Coding/Trilithon/docs/todo/phase-05-snapshot-writer.md (706 lines)
**Slices analysed:** 7

---

## Proposed Tags

### 5.1: Canonical JSON serialiser with CANONICAL_JSON_VERSION
**Proposed tag:** [cross-cutting]
**Reasoning:** Although all files live in `core`, this slice introduces a project-wide convention (CANONICAL_JSON_VERSION + deterministic byte form) that every downstream snapshot, hash, and audit-log entry must conform to. ADR-0009's content-addressing invariant structurally depends on this format, and slices 5.2, 5.5, 5.7 plus future phases (7/8+) all consume it. A versioned canonical form is exactly the "convention other slices must follow" trigger in the rubric.
**Confidence:** high

### 5.2: Content-address helper (SHA-256 hex)
**Proposed tag:** [trivial]
**Reasoning:** Single helper function in one module (`core/mutation/content_address.rs`), no I/O, no new trait, no shared type modified. It depends on 5.1's convention but does not itself introduce one — it is a thin wrapper returning a `SnapshotId` newtype. Fits the "single helper function" trivial example exactly.
**Confidence:** high

### 5.3: `Snapshot` record finalised with created_at_monotonic_nanos
**Proposed tag:** [cross-cutting]
**Reasoning:** Modifies the shared `core::storage::Snapshot` struct that both `InMemoryStorage` and `SqliteStorage` (Phase 2) implement against, AND ships a paired adapters-layer SQL migration (`0003_snapshot_monotonic_nanos.sql`). Field addition to a shared type + cross-crate (core + adapters) file changes + downstream-depended migration hits three rubric triggers.
**Confidence:** high

### 5.4: Migration 0004_snapshots_immutable.sql (UPDATE/DELETE triggers)
**Proposed tag:** [cross-cutting]
**Reasoning:** Single-file DDL, but the rubric explicitly calls out "migration that other slices structurally depend on" as cross-cutting. These triggers establish the database-level immutability invariant from ADR-0009 that every subsequent snapshot write (5.5, Phase 7+) and all audit-log behaviour relies on. It is a structural constraint, not a localised change.
**Confidence:** high

### 5.5: SnapshotWriter adapter — dedupe, parent linkage, monotonic version
**Proposed tag:** [cross-cutting]
**Reasoning:** Crosses the core↔adapters boundary by importing `DesiredState`, canonical_json, content_address, and Storage types from core, and implements write logic enforcing the ADR-0012 monotonic-version optimistic-concurrency contract plus ADR-0009 dedupe/parent linkage. References 2+ ADRs and ties together the conventions introduced in 5.1/5.3/5.4 — meets the layer-crossing and multi-ADR triggers.
**Confidence:** medium
**If low confidence, why:** Borderline — the slice description says it "stays in adapters layer," which could read as standard; tagged cross-cutting because it crosses the core→adapters import boundary and binds multiple ADR invariants together.

### 5.6: Snapshot fetch operations (id, version, parent, date range)
**Proposed tag:** [standard]
**Reasoning:** Self-contained read API in one adapters file, no new trait, no shared type modified, no migration, no new convention. Adds I/O (SQL queries) within a single adapter — the canonical "query API in one module" standard example.
**Confidence:** high

### 5.7: Property tests, canonical-json corpus, monotonicity, root-NULL invariant
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span both `core` and `adapters` crates (tests + fixtures + README modification), and the slice locks in invariants from ADR-0009 (immutability, root-NULL, content-addressing) and ADR-0012 (monotonic version). The 50 checked-in fixture pairs become the canonical-form contract every future change must respect — multi-crate scope plus multi-ADR coverage trigger cross-cutting.
**Confidence:** high

---

## Summary
- 1 trivial
- 1 standard
- 5 cross-cutting
- 1 low-confidence (require human review)

## Notes
- Phase 5 is foundation-laying for the snapshot/audit subsystem, so cross-cutting density is expected: it ships two migrations, modifies a shared core type, and introduces the canonical-JSON convention that downstream phases hash against.
- 5.1, 5.3, and 5.4 each independently establish a structural contract (format version, schema field, immutability triggers); 5.5 and 5.7 then bind those contracts together. Sequencing matters — 5.1→5.2, 5.3→5.4→5.5, then 5.6/5.7.
- 5.2 and 5.6 are the only genuinely localised slices; everything else has a layer-boundary or invariant-establishing dimension.
- 5.5's confidence is medium because the file inventory is single-crate (adapters) but the *semantic* surface crosses layers via core imports and binds ADR-0009 + ADR-0012 — recommend a quick human sanity check on whether the team treats core-import-only as cross-cutting.

---

## User Decision
**Date:** 2026-05-05
**Decision:** accepted

### Modifications
None.

### Notes from user
None.

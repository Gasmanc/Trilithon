# Phase 8 — Tagging Analysis
**Generated:** 2026-05-10
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md (project instructions)
- architecture.md (1124 lines) — focused on §6.6, §7.2, §9, §12.1
- trait-signatures.md (734 lines) — focused on §5 DiffEngine, §1 Storage
- PRD — referenced via phase doc cross-references (T1.4)
- phase-08-drift-detection.md (112 lines)
- ADR-0002 (133 lines)
- ADR-0009 (185 lines)
- phase-08 TODO / slices (643 lines)
- seams.md (98 lines) — `seams: []`
- seams-proposed.md (31 lines) — `proposed_seams: []`
- contract-roots.toml (41 lines) — no roots defined

**Slices analysed:** 6

---

## Proposed Tags

### 8.1: `DiffEngine` structural diff over canonical JSON
**Proposed tag:** [standard]
**Reasoning:** Introduces the `DiffEngine` trait (trait-signatures.md §5) with three methods (`structural_diff`, `apply_diff`, `redact_diff`) plus `DefaultDiffEngine`, `Diff`, `DiffEntry`, `DiffError`, and `JsonPointer` types. All files are within `core` (`diff.rs`, `diff/flatten.rs`, `lib.rs`). Pure-core, no I/O, no async. Tagged standard rather than trivial because it introduces a new trait listed in the trait-signatures registry and implements a non-trivial algorithm (flatten + set-difference + apply-inverse).
**Affected seams:** PROPOSED: `diff-engine-storage` (the DiffEngine output feeds into Storage::record_drift_event in later slices; this is the producer side of that seam)
**Planned contract additions:** `trilithon_core::diff::DiffEngine`, `trilithon_core::diff::DefaultDiffEngine`, `trilithon_core::diff::Diff`, `trilithon_core::diff::DiffEntry`, `trilithon_core::diff::DiffError`, `trilithon_core::diff::JsonPointer`
**Confidence:** high

### 8.2: Caddy-managed-paths ignore list (architecture §7.2)
**Proposed tag:** [trivial]
**Reasoning:** Single submodule (`diff/ignore_list.rs`) with a static constant table and one pure function `is_caddy_managed`. No new traits, no cross-layer dependency, no I/O. The only integration point is a call site in `structural_diff` (same crate, same module tree). Regex compilation uses `once_cell::sync::Lazy` but this is internal to the module.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::diff::ignore_list::is_caddy_managed`, `trilithon_core::diff::ignore_list::CADDY_MANAGED_PATH_PATTERNS`
**Confidence:** high

### 8.3: `DriftEvent`, `DiffCounts`, and `DesiredState::unknown_extensions` round-trip
**Proposed tag:** [standard]
**Reasoning:** Adds `DriftEvent`, `DiffCounts`, `ObjectKind` types to `core::diff` and extends `DesiredState` in `core::desired_state` with the `unknown_extensions` field. Touches two modules within the same crate (core). The `DriftEvent` struct is consumed by `Storage::record_drift_event` (trait-signatures.md §1), making it a contract-surface type. The `ObjectKind` classifier and canonical-JSON byte-stability property test add non-trivial logic. Not cross-cutting because all changes remain within `core`.
**Affected seams:** PROPOSED: `diff-engine-storage` (DriftEvent is the payload shape flowing across the diff-to-storage boundary)
**Planned contract additions:** `trilithon_core::diff::DriftEvent`, `trilithon_core::diff::DiffCounts`, `trilithon_core::diff::ObjectKind`
**Confidence:** high

### 8.4: Three resolution APIs in core: adopt, reapply, defer
**Proposed tag:** [standard]
**Reasoning:** New module `core::diff::resolve` with three pure functions producing `Mutation` variants. Depends on Phase 4's `Mutation` enum (confirmed at `core/crates/core/src/mutation/types.rs`) — this slice adds new variants (`ReplaceDesiredState`, `ReapplySnapshot`, `DriftDeferred`) to the existing enum or consumes existing variants. All files within `core`. No I/O, no async. Standard because it extends the `Mutation` enum surface and introduces `ResolveError`.
**Affected seams:** none (the Mutation variants flow through existing apply infrastructure)
**Planned contract additions:** `trilithon_core::diff::resolve::adopt_running_state`, `trilithon_core::diff::resolve::reapply_desired_state`, `trilithon_core::diff::resolve::defer_for_manual_reconciliation`, `trilithon_core::diff::resolve::ResolveError`
**Confidence:** high

### 8.5: `DriftDetector` scheduler with `tokio::time::interval` and apply-in-flight skip
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span three crates: `adapters/src/drift.rs`, `adapters/src/lib.rs`, and `cli/src/run.rs`. Crosses two layer boundaries (core types consumed by adapters, adapters wired into cli). Shares `apply_mutex: Arc<tokio::sync::Mutex<()>>` with Phase 7's applier — a concurrency coordination point across phases. The CLI wiring spec modifies `run_with_shutdown` startup ordering (sentinel, capability probe, then drift detector). Emits `drift.detected` and `drift.skipped` tracing events per architecture §12.1. References ADR-0002, ADR-0009, architecture §7.2, §9, §12.1, and trait-signatures §1 and §5.
**Affected seams:** PROPOSED: `drift-detector-applier` (shared apply_mutex between drift detector and Phase 7 applier is a cross-phase coordination seam)
**Planned contract additions:** `trilithon_adapters::drift::DriftDetector`, `trilithon_adapters::drift::DriftDetectorConfig`, `trilithon_adapters::drift::TickOutcome`, `trilithon_adapters::drift::TickError`
**Confidence:** high

### 8.6: Drift audit row writer plus deduplication per cycle
**Proposed tag:** [standard]
**Reasoning:** Extends `DriftDetector` in `adapters/src/drift.rs` with `record()` and `mark_resolved()` methods. All changes within the adapters crate. Emits `config.drift-detected` and `config.drift-resolved` audit kinds (architecture §6.6) and corresponding tracing events. The `mark_resolved` method is designed to be called by Phase 9 resolution endpoints but does not itself cross into cli. The deduplication logic (hash-based skip) is self-contained. Standard rather than cross-cutting because changes are confined to adapters and the audit event vocabulary is pre-defined in §6.6.
**Affected seams:** PROPOSED: `drift-detector-storage` (record() writes through Storage::record_drift_event and AuditWriter::record)
**Planned contract additions:** `trilithon_adapters::drift::DriftDetector::record`, `trilithon_adapters::drift::DriftDetector::mark_resolved`, `trilithon_adapters::drift::ResolutionKind`
**Confidence:** high

---

## Summary
- 1 trivial
- 4 standard
- 1 cross-cutting
- 0 low-confidence

## Notes

**Cross-slice dependency chain:** 8.1 → 8.2 (ignore list wired into diff) → 8.3 (types) → 8.4 (resolvers) → 8.5 (scheduler) → 8.6 (audit writer). Strictly linear; no parallelism opportunities.

**Shared mutex pattern:** Slice 8.5 shares `apply_mutex` with Phase 7's applier. This is the only cross-phase runtime coordination point in Phase 8 and is the primary driver of the cross-cutting tag.

**New Mutation variants:** Slice 8.4 adds `ReplaceDesiredState`, `ReapplySnapshot`, and `DriftDeferred` to the `Mutation` enum. These must be handled by the Phase 7 applier's match arms (likely as no-ops or pass-throughs for `DriftDeferred`).

**Proposed seams:** Three seams are proposed: `diff-engine-storage` (DiffEngine output to Storage), `drift-detector-applier` (shared mutex), and `drift-detector-storage` (audit writes). All are genuinely new boundaries not covered by the empty seam registry.

**Architecture §12.1 update:** Slice 8.5 flags that `drift.skipped` should be added to the tracing vocabulary. The current §12.1 table already lists `drift.skipped` but with a different description ("Caddy unreachable" vs "apply-in-flight"). The slice's description should align or a second event name should be used.

---

## User Decision
**Date:** 2026-05-10
**Decision:** accepted

### Modifications (if any)
None.

### Notes from user
None.

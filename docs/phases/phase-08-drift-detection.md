# Phase 8 — Drift detection loop

Source of truth: [`../phases/phased-plan.md#phase-8--drift-detection-loop`](../phases/phased-plan.md#phase-8--drift-detection-loop).

> **Path-form note.** All `crates/<name>/...` paths are workspace-relative; rooted at `core/` on disk. See [`README.md`](README.md) "Path conventions". Phase 8 introduces `crates/core/src/diff.rs` (the engine, pure logic) and `crates/adapters/src/drift.rs` (the scheduler task using `tokio::time::interval`).

> **Authoritative cross-references.** The `core::diff::DiffEngine` trait surface is documented in [`../architecture/trait-signatures.md`](../architecture/trait-signatures.md). Audit kinds emitted (`config.drift-detected`, `config.drift-resolved`) are bound by architecture §6.6. Tracing events emitted (`drift.detected`, `drift.resolved`) are bound by architecture §12.1. The Caddy-managed-paths ignore list lives at architecture §7.2.

## Pre-flight checklist

- [ ] Phase 7 complete (apply path is operational).

## Tasks

### Backend / core crate

- [ ] **Implement the structural diff engine.**
  - Path: `crates/core/src/diff.rs`.
  - Acceptance: The engine MUST implement `core::diff::DiffEngine` (see [`../architecture/trait-signatures.md`](../architecture/trait-signatures.md)). The structural-diff algorithm MUST be implemented as numbered pseudocode:

    ```
    1. Flatten each side to BTreeMap<JsonPointer, JsonValue>:
       `flat_a = flatten(state_a.canonical_json())`,
       `flat_b = flatten(state_b.canonical_json())`.
    2. Compute key sets `keys_a`, `keys_b`. Symmetric diff into `added`, `removed`.
    3. For each key in `keys_a ∩ keys_b`, if `flat_a[k] != flat_b[k]`, classify as `Modified { before, after }`.
    4. Discard any entry whose JsonPointer matches the Caddy-managed-paths ignore list (architecture §7.2): TLS issuance state, upstream health caches, automatic_https.disable_redirects, request_id placeholders.
    5. Return `Diff { added, removed, modified, ignored_count }`.
    ```
  - Done when: `cargo test -p trilithon-core diff::tests` covers add, remove, modify, unchanged, and every entry in the architecture §7.2 ignore list.
  - Feature: T1.4 substrate.
- [ ] **Define the `DriftEvent` record.**
  - Path: `crates/core/src/diff.rs` (alongside the engine).
  - Acceptance: The Rust definitions MUST appear verbatim:

    ```rust
    pub struct DriftEvent {
        pub before_snapshot_id: SnapshotId,
        pub running_state_hash: String,            // SHA-256 of fetched JSON
        pub diff_summary:       BTreeMap<ObjectKind, DiffCounts>,
        pub detected_at:        UnixSeconds,
        pub correlation_id:     Ulid,
    }
    pub struct DiffCounts { pub added: u32, pub removed: u32, pub modified: u32 }
    ```
  - Done when: serde round-trip and unit-test coverage are present.
  - Feature: T1.4.
- [ ] **Implement the three resolution APIs in core.**
  - Acceptance: Pure-core APIs MUST expose `adopt_running_state`, `reapply_desired_state`, and `defer_for_manual_reconciliation`, each producing exactly one mutation.
  - Done when: unit tests cover every transition and assert exactly one mutation per call.
  - Feature: T1.4.
- [ ] **Preserve unknown extensions when ingesting running state.**
  - Acceptance: `DesiredState` MUST carry `pub unknown_extensions: BTreeMap<JsonPointer, serde_json::Value>` for fields that pass through Caddy unmodified. The canonical-JSON serialiser MUST sort keys lexicographically so round-trips are byte-stable.
  - Done when: a unit test with a hand-written unknown field asserts round-trip preservation and a property test asserts byte-stability of the canonical-JSON serialisation under key reordering.
  - Feature: T1.4.

### Backend / adapters crate

- [ ] **Schedule the `DriftDetector` task.**
  - Path: `crates/adapters/src/drift.rs`.
  - Acceptance: The daemon MUST run the detector once at startup and every `drift_check_interval_seconds` (default 60) thereafter via `tokio::time::interval`. Each tick emits a span; non-empty diffs emit `tracing::info!(target = "drift.detected", ...)` per architecture §12.1.
  - Done when: an integration test observes ticks at the configured interval.
  - Feature: T1.4.
- [ ] **Skip a tick if an apply is in flight.**
  - Acceptance: Detection MUST skip a tick if an apply is in flight; the skip MUST emit a tracing event but no audit row.
  - Done when: an integration test asserts the skip and the tracing event.
  - Feature: T1.4.
- [ ] **Persist `DriftDetected` via the audit writer.**
  - Acceptance: A non-empty diff MUST produce exactly one `DriftDetected` audit row per detection cycle until resolved.
  - Done when: an integration test that induces drift via an out-of-band Caddy mutation observes one audit row per cycle.
  - Feature: T1.4.

### Tests

- [ ] **Clean-state silence.**
  - Acceptance: A clean state MUST produce no drift event.
  - Done when: an integration test asserts zero `DriftDetected` rows over five ticks against an unchanged Caddy.
  - Feature: T1.4.
- [ ] **Out-of-band mutation triggers exactly one event.**
  - Acceptance: An out-of-band Caddy mutation MUST produce exactly one drift event per detection cycle until resolved.
  - Done when: the integration test passes.
  - Feature: T1.4.
- [ ] **Resolution paths each transition the event to resolved.**
  - Acceptance: Each of the three resolution paths MUST be exercised by integration tests and MUST write a `DriftResolved` audit row.
  - Done when: three integration tests cover the three paths.
  - Feature: T1.4.
- [ ] **Default interval is 60 seconds and configuration-overridable.**
  - Acceptance: The default detection interval MUST be 60 seconds and MUST be configuration-overridable.
  - Done when: a unit test asserts the default; an integration test asserts override behaviour.
  - Feature: T1.4.

### Documentation

- [ ] **Document the drift detector and the three resolution paths.**
  - Acceptance: `core/README.md` MUST describe the detector schedule, the three resolution paths, and the "never silently overwrite" invariant.
  - Done when: the section exists.
  - Feature: T1.4.

## Cross-references

- ADR-0002 (Caddy JSON admin API as source of truth).
- ADR-0009 (immutable content-addressed snapshots and audit log).
- PRD T1.4 (drift detection on startup and on schedule).
- Architecture: "Drift detection," "Resolution paths," "Failure modes — running state diverged."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] A non-empty diff between desired and running state produces exactly one `DriftDetected` audit row per detection cycle until resolved.
- [ ] Drift detection does not silently overwrite Caddy.
- [ ] The three resolution paths are implemented and exercised by integration tests.
- [ ] The default detection interval is 60 seconds and is configuration-overridable.

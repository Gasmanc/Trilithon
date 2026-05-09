## Slice 7.5
**Status:** complete
**Date:** 2026-05-09
**Commit:** a4921b7
**Summary:** Added optimistic concurrency control to the apply path. `StorageError::OptimisticConflict` added to core error types. `Storage` trait gained `current_config_version` and `cas_advance_config_version` methods, implemented on both `InMemoryStorage` and `SqliteStorage` (SQLite impl uses `BEGIN IMMEDIATE` for TOCTOU safety). `CaddyApplier::apply` now performs a CAS check before executing: on conflict it writes a `mutation.conflicted` audit row and returns `Ok(ApplyOutcome::Conflicted)` (typed outcome, not Err). Three integration tests cover the stale-version rejection, pointer-unchanged-on-conflict, and two-actor race scenarios. Gate green.

### Simplify Findings
1. `applier_caddy.rs` match arm on `OptimisticConflict` used a full-path qualifier (`trilithon_core::storage::error::StorageError::OptimisticConflict`) when `StorageError` was already importable — added `use` import and simplified the arm.
2. `InMemoryStorage::current_config_version` and `cas_advance_config_version` filtered snapshots using `s.actor.is_empty()` as a proxy for `caddy_instance_id` (Snapshot has no such field) — removed the spurious filter; V1 is single-instance so all values are iterated directly.

### Items Fixed Inline
- Added `StorageError` import to `applier_caddy.rs`; simplified match arm from full-path to `StorageError::OptimisticConflict`.
- Removed incorrect `instance_id == "local" || s.actor.is_empty()` filter from `InMemoryStorage` CAS methods; replaced parameter with `_instance_id`.

### Items Left Unfixed
none

## Slice 7.4
**Status:** complete
**Date:** 2026-05-09
**Commit:** 7301531
**Summary:** Created `core/crates/adapters/src/applier_caddy.rs` implementing `core::reconciler::Applier` as `CaddyApplier`. Covers render → capability re-check → POST /load → GET /config equivalence check → audit row (happy path); maps Caddy 4xx to `Ok(ApplyOutcome::Failed{CaddyValidation})` with `config.apply-failed` audit row; maps Caddy unreachable to `Err(ApplyError::Unreachable)` with `caddy.unreachable` audit row; maps post-load equivalence failure to `Err(ApplyError::CaddyRejected)`. Added `DiffEngine` / `NoOpDiffEngine` in `core::diff`. Four integration test files cover all four code paths (8 tests total); gate green.

### Simplify Findings
nothing flagged

### Items Fixed Inline
none

### Items Left Unfixed
none

## Slice 7.3
**Status:** complete
**Date:** 2026-05-09
**Summary:** Created `core/crates/core/src/reconciler/capability_check.rs` with the pure `check_against_capability_set` function. Walks all enabled routes in a `DesiredState`, derives required modules from upstreams/redirects/headers/policy-preset body keys, and checks each against the live `CapabilitySet`. All five required tests plus two additional edge-case tests pass; gate green.

### Simplify Findings
nothing flagged

### Items Fixed Inline
- Removed unused `BTreeSet` import leftover from initial draft (caught by `cargo clippy`).

### Items Left Unfixed
none

## Slice 7.2
**Status:** complete
**Date:** 2026-05-09
**Summary:** Created `core/crates/core/src/reconciler/applier.rs` with all five apply-state types (`ApplyOutcome`, `AppliedState`, `ReloadKind`, `ApplyFailureKind`, `ApplyError`). Landed `ReloadKind::Graceful { drain_window_ms: Option<u32> }` as the final shape to avoid within-phase churn at slice 7.7. All serde round-trip and exhaustive-variant tests pass; `just check` green.

### Simplify Findings
1. `applier.rs` doc comment on `ApplyError` referenced a non-existent `ApplyOutcome::from_error` method — removed the stale forward reference and replaced with a generic description.

### Items Fixed Inline
- Removed stale `ApplyOutcome::from_error` forward-reference from `ApplyError` doc comment.

### Items Left Unfixed
none

## Slice 7.1
**Status:** complete
**Date:** 2026-05-09
**Summary:** Implemented `core::reconciler` with `CaddyJsonRenderer` trait, `DefaultCaddyJsonRenderer`, and `canonical_json_bytes`. Added `unknown_extensions: BTreeMap<JsonPointer, Value>` to `DesiredState`. All 6 spec tests pass; insta snapshots committed for three fixture states.

### Simplify Findings
1. `canonicalise` function in `render.rs` duplicated `canonicalise_value` in `canonical_json.rs` — removed the duplicate, promoted `canonicalise_value` to `pub(crate)`, and reused it.
2. `validate_hostname_for_render` was a one-line thin wrapper over `crate::model::route::validate_hostname` — inlined at the call site.

### Items Fixed Inline
- Removed duplicate `canonicalise` function (40 lines) in `render.rs`; reuses `canonical_json::canonicalise_value` instead.
- Inlined `validate_hostname_for_render` thin wrapper at its single call site.

### Items Left Unfixed
none

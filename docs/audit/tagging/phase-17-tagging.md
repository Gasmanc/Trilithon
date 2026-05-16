# Phase 17 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, docs/architecture/architecture.md, docs/architecture/trait-signatures.md, docs/planning/PRD.md, docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md, docs/adr/0012-optimistic-concurrency-on-monotonic-config-version.md (plus ADR index), docs/todo/phase-17-concurrency-control.md, docs/architecture/seams.md, docs/architecture/seams-proposed.md, docs/architecture/contract-roots.toml, docs/architecture/contracts.md
**Slices analysed:** 8

## Proposed Tags

### 17.1: Conflict and rebase types
**Proposed tag:** [standard]
**Reasoning:** Adds a new `concurrency` module to `crates/core` only — pure type surface (`ConflictError`, `ActorRef`, `MutationCommutativity`, `RebasePlan`, `ThreeWayDiff`, `FieldConflict`, `FieldResolution`, `FieldPath`, `RebaseError`, `RebaseToken`) with serde impls. No I/O, no async, no new trait, no cross-layer dep. It is more than [trivial] because it lands a substantial public type vocabulary that every later slice (and the `cli`/`web` tiers) consumes, and `FieldPath` carries hand-written `Serialize`/`Deserialize` impls — a wider blast radius than a one-off helper.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::concurrency::{ConflictError, ActorRef, MutationCommutativity, RebasePlan, ThreeWayDiff, FieldConflict, FieldResolution, FieldPath, RebaseError, RebaseToken}` — public wire-shape types consumed across the `cli` and `web` boundary; candidates for `contract-roots.toml`.
**Confidence:** high
**If low confidence, why:** n/a

### 17.2: Rebase planner (pure)
**Proposed tag:** [standard]
**Reasoning:** Adds `plan_rebase` in a new `concurrency/rebase.rs` submodule and extends `TypedMutation` with `effective_field_set` — both inside `crates/core` only. Pure logic, no I/O, no async, no trait change. It extends one existing type (`TypedMutation`) and stays within one crate, which is the [standard] profile; it is not [trivial] because adding a method to the shared `TypedMutation` type and shipping a proptest invariant is non-localised within `core`.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::concurrency::rebase::plan_rebase`, `trilithon_core::mutation::TypedMutation::effective_field_set`
**Confidence:** high
**If low confidence, why:** n/a

### 17.3: Three-way diff and resolution apply
**Proposed tag:** [standard]
**Reasoning:** Adds `apply_resolutions` to `concurrency/rebase.rs`, a new `concurrency/diff.rs` (`json_pointer_set`, `JsonPointerError`), and a `TypedMutation::with_field` method — all in `crates/core`, pure, no I/O, no async, no trait change. Single-crate, extends one existing type; matches the [standard] rubric. Not [trivial] because it touches the shared `TypedMutation` type and adds a second public submodule.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::concurrency::rebase::apply_resolutions`, `trilithon_core::concurrency::diff::{json_pointer_set, JsonPointerError}`, `trilithon_core::mutation::TypedMutation::with_field`
**Confidence:** high
**If low confidence, why:** n/a

### 17.4: Snapshot writer compare-and-swap and rebase-token store
**Proposed tag:** [cross-cutting]
**Reasoning:** Spans three crates — `crates/adapters` (`snapshot_store.rs` CAS extension, new `rebase_token_store.rs`), `crates/core` (`config.rs` `ConcurrencyConfig`, `error.rs` `ConfigError::OutOfRange`), and `crates/cli` (a daemon-refuses-invalid-TTL test) — and crosses the core↔adapters boundary. It modifies the snapshot-writer `insert_if_absent` boundary that the apply pipeline depends on and introduces the new `SnapshotWriteError::Conflict` carrying a `ConflictError`. This is the optimistic-concurrency CAS gate central to T2.10/ADR-0012/H8; downstream slices 17.5/17.6/17.8 depend on its conflict shape.
**Affected seams:** snapshots-config-version-cas (active; this slice realises the `ConflictError` payload at the CAS gate that seam's contracts describe)
**Planned contract additions:** `trilithon_adapters::snapshot_store::{SnapshotWriteError, SnapshotInsertOutcome, SnapshotStore::insert_if_absent}`, `trilithon_adapters::rebase_token_store::{RebaseTokenStore, RebaseTokenError}`, `trilithon_core::config::ConcurrencyConfig`, `trilithon_core::error::ConfigError::OutOfRange`
**Confidence:** medium
**If low confidence, why:** The CAS change may instead be modelled against the existing `Storage::cas_advance_config_version` contract rather than a fresh `insert_if_absent` signature; the exact seam-contract mapping is for `/phase-merge-review` to confirm.

### 17.5: Conflict HTTP envelope and audit kinds
**Proposed tag:** [cross-cutting]
**Reasoning:** Touches `crates/cli` (new `http/mutations/conflict.rs`, handler change) and `crates/core` (`audit.rs` adds five `AuditEvent` variants), crossing the cli↔core boundary. It introduces five new audit kinds (`mutation.conflicted`, `mutation.rebased.*`, `mutation.rebase.expired`, `mutation.rejected.missing-expected-version`) — an audit-vocabulary convention other slices and `architecture.md` §6.6 follow — and changes the shared 409 HTTP envelope for every mutation endpoint. New audit conventions plus a shared wire-contract change are squarely [cross-cutting].
**Affected seams:** applier-audit-writer (active; new `AuditEvent` variants extend the typed audit-row vocabulary this seam exercises)
**Planned contract additions:** `trilithon_core::audit::AuditEvent::{MutationConflicted, MutationRebasedAuto, MutationRebasedManual, MutationRebaseExpired, MutationRejectedMissingExpectedVersion}`; `trilithon_cli::http::mutations::conflict::{ConflictResponseBody, render_conflict}` (HTTP wire contract, typically not a registry root)
**Confidence:** high
**If low confidence, why:** n/a

### 17.6: `POST /api/v1/mutations/rebase` endpoint
**Proposed tag:** [standard]
**Reasoning:** Adds a single new handler (`http/mutations/rebase.rs`) and a router mount in `crates/cli`, composing existing `core` functions (`apply_resolutions`, `validate_mutation`) and existing `adapters` stores. It writes already-defined audit kinds — it introduces no new vocabulary, no new trait, no new tracing event. The work is contained in one crate (`cli`) wiring an HTTP surface over established primitives, which fits the [standard] "one adapter/entry crate, may add I/O" profile rather than [cross-cutting].
**Affected seams:** none (consumes the conflict/CAS seam surfaces realised in 17.4/17.5; introduces no new boundary)
**Planned contract additions:** `trilithon_cli::http::mutations::rebase::{RebaseRequest, RebaseResponseBody, submit_rebase}` — HTTP route surface; not a registry root.
**Confidence:** medium
**If low confidence, why:** The new public REST endpoint plus a `dry-run` companion route could be argued as a cross-phase integration boundary for the web tier, nudging toward [cross-cutting].

### 17.7: Conflict banner and rebase view (web UI)
**Proposed tag:** [standard]
**Reasoning:** Entirely within the `web/` TypeScript frontend — new `features/concurrency/*` and one shared `components/diff/ThreeWayDiff.tsx`, plus a router registration. No Rust, no crate boundary crossed, no trait, no audit/tracing convention. The TypeScript types mirror the slice 17.1 wire shapes but introduce no new backend contract. Self-contained UI feature in one module group — the [standard] profile.
**Affected seams:** none
**Planned contract additions:** none (TypeScript-only; no Rust contract surface)
**Confidence:** high
**If low confidence, why:** n/a

### 17.8: End-to-end concurrency scenarios
**Proposed tag:** [standard]
**Reasoning:** Six integration test files under `crates/adapters/tests/`. It adds no production code, no new types, no new public surface — it exercises the conflict/rebase path end-to-end against the daemon. Tests-only work that spans behaviour but introduces no contract or boundary is [standard]; it is not [cross-cutting] because it changes no shared trait, convention, or layer dependency, and not [trivial] because it drives a multi-actor, multi-crate scenario.
**Affected seams:** snapshots-config-version-cas, applier-audit-writer (exercised, not modified — these end-to-end scenarios validate the conflict CAS gate and audit-row emission)
**Planned contract additions:** none (tests only)
**Confidence:** high
**If low confidence, why:** n/a

## Summary
- 0 trivial / 6 standard / 2 cross-cutting / 2 low-confidence — across 8 slices.
- Trivial: 0
- Standard: 6 (17.1, 17.2, 17.3, 17.6, 17.7, 17.8)
- Cross-cutting: 2 (17.4, 17.5)
- Low-confidence: 2 (17.4, 17.6)

## Notes

- **No new seams proposed.** Phase 17 realises and exercises the existing `snapshots-config-version-cas` seam (the optimistic-concurrency CAS gate, introduced Phase 7) and extends the `applier-audit-writer` seam's audit vocabulary. The rebase HTTP endpoint and the in-memory `RebaseTokenStore` are new surfaces but sit inside established layer boundaries; `seams-proposed.md` stays empty. `/phase-merge-review` should confirm whether the new conflict-response wire contract warrants a seam entry for the cli↔web boundary.
- **Contract registry is currently empty** (`contracts.md` reports `contract_count: 0`; `contract-roots.toml` lists only Phase 7 `reconciler` roots). The slice 17.1 type surface (`ConflictError`, `RebasePlan`, etc.) and slice 17.4/17.5 additions are strong candidates for new `contract-roots.toml` entries — that addition is itself a contract change for `/phase-merge-review` to ratify.
- **Audit-vocabulary coupling.** Slice 17.5 must add the five new `mutation.*` kinds to `architecture.md` §6.6 *in the same commit* per the §6.6 "Vocabulary authority" rule, and the §6.6 table already pre-lists these kinds plus `config.rebased`. The `core::audit::AuditEvent` ↔ wire-`kind` mapping table in §6.6 must also gain the five `PascalCase` variants. This pre-registration in the architecture doc reduces drift risk but the same-commit obligation is what makes 17.5 cross-cutting.
- **No new tracing events.** Every slice reuses the §12.1 closed vocabulary (`http.request.*`, `apply.*`); no slice adds an event name, so the §12.1 same-commit rule is not triggered.
- **Dependency spine.** 17.1 → 17.2 → 17.3 (pure `core`), 17.1 → 17.4 (cross-layer CAS), 17.4 → 17.5 → 17.6 (HTTP), 17.6 → 17.7 (web), and 17.8 depends on 17.4/17.5/17.6. The two cross-cutting slices (17.4, 17.5) are the integration chokepoints and warrant the most adversarial review.
- ADR/PRD reference density: the phase as a whole cites ADR-0009, ADR-0012, T1.3, T1.7, T2.10, and H8 — well past the 3+ threshold — but that density concentrates in slices 17.4 and 17.5, consistent with their cross-cutting tags. The pure-`core` slices touch the same ADRs conceptually but do not themselves cross a layer.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.

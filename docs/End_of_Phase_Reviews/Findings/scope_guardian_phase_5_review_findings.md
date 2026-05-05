# Phase 5 — Scope Guardian Review Findings

**Reviewer:** scope_guardian
**Date:** 2026-05-05
**Diff range:** d4a320a..HEAD
**Phase:** 5

---

Scope verdict: too large
Coherence verdict: partially coherent

[HIGH] Web frontend changes have no corresponding Phase 5 work unit
File: web/eslint.config.js, web/pnpm-lock.yaml, web/tailwind.config.js, web/vite.config.ts
Lines: general
Description: The diff includes 4 changed web files (reformatted eslint.config.js, pnpm-lock.yaml with 4113 lines changed, minor changes to tailwind.config.js and vite.config.ts). None of Phase 5's work units mention the web frontend. These are unrelated pre-phase baseline fixes that crept into the phase commit range.
Question: Scope
Suggestion: These were baseline formatter fixes committed before phase work — excluded from review scope.

[HIGH] Canonical hash not computed in the write path — snapshot_id accepted verbatim from caller
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 336-430
Description: The TODO acceptance criterion states "`SnapshotWriter` MUST compute the canonical hash." The diff does not call `canonical_json::content_address` anywhere inside `insert_snapshot`. The `snapshot_id` value is taken verbatim from the Snapshot struct; there is no recomputation or verification of the id against `desired_state_json`.
Question: Coherence
TODO unit: Implement the `SnapshotWriter` adapter — MUST compute the canonical hash
Suggestion: Inside `insert_snapshot`, recompute the SHA-256 content address from `snapshot.desired_state_json` and assert it matches `snapshot.snapshot_id.0` before the deduplication check.

[WARNING] SnapshotWriter not a named struct — rolled into SqliteStorage
File: core/crates/adapters/src/sqlite_storage.rs
Lines: general
Description: The TODO specifies "Implement the `SnapshotWriter` adapter" as a named type. The diff implements all required logic directly as methods on SqliteStorage rather than as a distinct SnapshotWriter struct. No named SnapshotWriter type exists anywhere post-diff.
Question: Coherence
Suggestion: Either introduce a SnapshotWriter newtype wrapping SqliteStorage, or clarify in the phase doc that SnapshotWriter refers to the insert_snapshot implementation on SqliteStorage.

[WARNING] caddy_instance_id hardcoded to 'local' in monotonicity query without comment
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 389
Description: The monotonicity check queries `WHERE caddy_instance_id = 'local'` as a string literal. The TODO specifies "strict monotonic increase per caddy_instance_id". The hardcoding is done silently without a comment referencing that decision or a tracked suppression id.
Question: Coherence
Suggestion: Add an inline comment on both hardcoded 'local' occurrences referencing the V1 single-instance decision.

[WARNING] Monotonicity property test uses loop-based approach, not proptest
File: core/crates/adapters/tests/snapshot.rs
Lines: 608-705
Description: The TODO says "A property test MUST assert strict monotonic increase per instance across interleaved writes." The test is a sequential deterministic loop, not a randomised property test. The module is named `props` but no proptest crate is used.
Question: Coherence
Suggestion: Either add proptest and convert to a proptest! macro, or explicitly document that a deterministic exhaustive loop was accepted as the substitute.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-05 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Web frontend changes have no Phase 5 work unit | 🔕 Superseded | — | — | — | Self-noted by scope_guardian as baseline formatter fixes; excluded from scope |
| 2 | Canonical hash not computed in the write path | ✅ Fixed | pre-review | — | 2026-05-05 | validate_snapshot_invariants already recomputes SHA-256 and checks |
| 3 | SnapshotWriter not a named struct | 🚫 Won't Fix | — | — | — | Methods on SqliteStorage is the accepted Phase 5 approach |
| 4 | caddy_instance_id hardcoded to 'local' without comment | 🚫 Won't Fix | — | — | — | V1 design; inline comments reference ADR-0009 |
| 5 | Monotonicity property test uses loop, not proptest | 🚫 Won't Fix | — | — | — | Comment documents loop-based approach as accepted substitute |

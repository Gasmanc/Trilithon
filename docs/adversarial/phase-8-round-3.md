# Phase 8 Adversarial Review — Round 3

**Date:** 2026-05-08
**Severity summary:** 3 critical · 4 high · 3 medium · 1 low

---

## New Findings (Round 3)

### F026 — Sentinel Removal Silently Ignored by `/storage/.*` Ignore Rule [CRITICAL]

**Category:** Logic flaws

**Attack:** The ignore list's second regex, `^/storage/.*`, matches `/storage/trilithon-owner` exactly — the path of the ownership sentinel. When the sentinel is removed out-of-band, the `Removed` entry for that path is silently discarded by `is_caddy_managed()`. `tick_once` returns `Clean`. No `DriftEvent` is created. No audit row is written. The drift loop provides zero detection of ownership loss.

This directly invalidates the assumption in round 2 F023, which assumed the sentinel removal *would* appear as `ObjectKind::Other` drift. It won't — the ignore list kills it first.

**Scenario:**
1. An operator (or competing Trilithon daemon) removes `/storage/trilithon-owner` from Caddy directly.
2. A second Trilithon daemon acquires ownership and writes its own sentinel.
3. `tick_once` fires. Diff contains `Removed { path: "/storage/trilithon-owner" }`.
4. `is_caddy_managed("/storage/trilithon-owner")` → true (matches `^/storage/.*`). Entry discarded.
5. `ignored_count` increments by 1. No other diff entries. `tick_once` returns `Clean`.
6. Trilithon continues applying mutations to a Caddy instance it no longer owns, with no awareness ADR-0015's protection is gone.

**Design gap:** The `/storage/.*` rule is documented as covering Caddy's TLS certificate automation paths (`/storage/acme/`, `/storage/ocsp/`). It is over-broad and accidentally covers the sentinel path that Trilithon itself owns and must not ignore. There is no carve-out for `/storage/trilithon-owner`.

---

### F027 — `record()` Error Is Unobservable — Drift Event Silently Dropped [CRITICAL]

**Category:** Partial failure atomicity

**Attack:** `tick_once` returns `Drifted { event }`. `run()` calls `record(event)`. `record()` writes to SQLite via `storage.record_drift_event(event)`. If the write fails (disk full, WAL lock, I/O timeout), `record()` returns `Err`. The design specifies no error handling for this case in `run()`.

**Scenario:**
1. Drift detected. `tick_once` returns `Drifted { event }`.
2. `record(event)` starts. Acquires `last_running_hash` lock.
3. SQLite write fails at step 4. `record()` returns `Err` before step 5 (hash update).
4. `last_running_hash` remains `None`.
5. If `run()` logs-and-continues, next tick detects same drift, calls `record()` again — retry is implicit but unspecified. If SQLite is still unavailable, this repeats indefinitely with no circuit breaker.
6. If `run()` short-circuits on error and sets the hash anyway to suppress log spam, the drift event is permanently lost with no audit trail.

**Design gap:** The design specifies `record()` returns `Result<(), ...>` but does not specify the caller's error handling. A single transient SQLite failure either causes an uncontrolled retry loop or permanently drops a drift event. Neither is acceptable for an immutable audit system.

---

### F028 — Caddy Plugin Fields Produce Permanent False-Positive Drift on Every Tick [CRITICAL]

**Category:** Assumption violation

**Attack:** `tick_once` parses the Caddy running state and places unknown fields in `unknown_extensions`. The desired state from storage was written by Trilithon and has empty `unknown_extensions`. `structural_diff` compares both flat maps: plugin-added paths (e.g., `/apps/crowdsec/api_key`) appear in the running map but not the desired map → `Added` entries on every tick. The ignore list has no coverage for third-party plugin namespaces.

**Scenario:**
1. Operator installs the `crowdsec` Caddy plugin. It writes state under `/apps/crowdsec/`.
2. Trilithon's desired state has no `/apps/crowdsec` key.
3. Every `tick_once`: diff contains `Added { path: "/apps/crowdsec/..." }`. `running_state_hash` changes if the plugin updates its state.
4. New `config.drift-detected` rows written continuously — one per hash change.
5. Operators see a permanent drift storm. Real drift is buried in noise. Operators disable alerting.

**Design gap:** The ignore list is a static allowlist of specific Caddy-internal paths. It has no mechanism for "paths outside Trilithon's management scope." There is no operator-configurable `external_roots` equivalent, and the `unknown_extensions` field was introduced precisely for this case — but `structural_diff` diffs through `canonical_json()` which either includes or excludes them (F012 round 1), and neither answer is correct without a management-scope boundary.

---

### F029 — `latest_desired_state()` May Return Unapplied Snapshot After Failed Apply — False Drift [HIGH]

**Category:** Eventual consistency

**Attack:** If the apply path writes a new snapshot to SQLite before confirming the Caddy push succeeds, and the Caddy push fails, SQLite holds a snapshot Caddy has never seen. The next `tick_once` compares this unapplied snapshot against Caddy's actual state. The diff looks like external drift but is Trilithon-internal inconsistency.

**Scenario:**
1. Desired state S1 is in SQLite and Caddy. Mutation M is queued.
2. Apply path writes S2 to SQLite, then calls `POST /load` on Caddy.
3. Caddy returns 502. S2 is in SQLite; Caddy holds S1.
4. Drift detector fires. `latest_desired_state()` → S2. Caddy → S1. Diff: all changes in M appear as drift.
5. Operator calls `adopt_running_state()` to resolve — regresses to S1, permanently losing mutation M.

**Design gap:** The design assumes `latest_desired_state()` returns the last *successfully applied* snapshot. It does not specify the write ordering in the apply path. If SQLite is written before Caddy confirmation, there is always a window where `latest_desired_state()` returns an unapplied snapshot and drift detection misclassifies it as external change.

---

### F030 — `Diff` Is Deserializable — Crafted Diff Submitted via Resolution API Bypasses Business Rules [HIGH]

**Category:** Trust boundary violations

**Attack:** `Diff` derives `serde::Deserialize`. Any endpoint that accepts a `Diff` from external input (tool-gateway, future `apply_patch` endpoint) allows arbitrary path mutation in `DesiredState`. A crafted diff with `Removed { path: "/storage/trilithon-owner" }` would self-evict the ownership sentinel when passed through `apply_diff`.

**Scenario:**
1. A tool-gateway token with `drift:resolve` permission calls an endpoint accepting a `Diff` body.
2. The body contains `Removed { path: "/storage/trilithon-owner" }`.
3. `apply_diff(&current_desired, &crafted_diff)` removes the sentinel from `DesiredState`.
4. `POST /load` confirms the sentinel-free config. A competing daemon acquires ownership.

**Design gap:** `Diff` being deserializable makes it a capability: whoever can submit one can rewrite arbitrary `DesiredState` paths. Diffs must be produced only by `structural_diff` from validated snapshots, never from external input. This is distinct from F002: F002 covers the redaction barrier; F030 covers the deserialization attack surface.

---

### F031 — `DiffCounts` u32 Wraps Silently on Full Config Wipe in Release Builds [HIGH]

**Category:** Boundary condition exploits

**Attack:** `DiffCounts { added: u32, removed: u32, modified: u32 }`. Rust's release builds use wrapping arithmetic on overflow. A full Caddy config wipe with 100,000 routes and 50,000 upstreams produces `removed` counts well within u32 range individually, but downstream consumers that sum across `ObjectKind` buckets, or a future version of `DiffCounts` that aggregates totals, can reach u32 overflow. More immediately: if the `DiffCounts` `total()` helper is implemented as `added + removed + modified` (a natural addition), three fields of ~1.4B each overflow at 4.29B total.

**Design gap:** The overflow behavior of `DiffCounts` arithmetic is unspecified. Release builds wrap silently. An operator viewing the audit UI could see `total changes: 5` on a full config wipe, providing false assurance that the drift is minor.

---

### F032 — No Integration Test Spans the Full `tick_once` → `record()` → Resolution → `mark_resolved` Path [HIGH]

**Category:** Observability gaps

**Attack:** Each slice specifies its own acceptance tests. No test spans the complete detection-to-resolution cycle. The following invariants are never tested end-to-end:
- `before_snapshot_id` in the `DriftEvent` matches the snapshot `reapply_desired_state` will push.
- `last_running_hash` is cleared by `mark_resolved` and the next tick returns `Clean`.
- A concurrent apply during resolution does not produce a false second drift event.

**Design gap:** The multi-step sequence (detect → record → resolve → mark_resolved → next-tick-clean) is Phase 8's primary correctness property. It is tested only piecemeal at slice boundaries. A regression in any one slice that breaks the invariant will not surface until production.

---

### F033 — `canonical_json()` of Full Running State May Exceed 200 ms Budget [MEDIUM]

**Category:** Backpressure and resource exhaustion

**Attack:** Architecture §13 requires `tick_once` < 200 ms for a 100-route config. Step 5 computes `SHA-256(canonical_json(running))` over the full `DesiredState`. For a 100-route config, the Caddy JSON response can be 1–2 MB. `canonical_json()` performs recursive key sorting and full serialization before hashing. Under allocator contention (concurrent request handling), this step alone can consume 60–120 ms, leaving the remaining steps (structural_diff, redact_diff, network) with insufficient headroom.

**Design gap:** The hash is computed over the full state when only the diff's fingerprint is needed for deduplication. Hashing the flattened `BTreeMap` produced during `structural_diff` (O(changed leaves)) instead of the full `DesiredState` (O(full config)) would stay within the 200 ms budget at any realistic config size.

---

### F034 — `DriftEvent` Exists in Memory Between `tick_once` Return and `record()` Call; Lost on SIGTERM [MEDIUM]

**Category:** Partial failure atomicity

**Attack:** `tick_once` returns `Drifted { event }` to `run()`. `run()` then calls `record(event)`. Between these two steps, if the process receives SIGTERM, the `DriftEvent` is dropped. `last_running_hash` is still `None` (the hash was never written). On restart, the next tick re-detects the same drift and writes a new row — functionally correct but producing a different `correlation_id`. Any operator holding the pre-crash `correlation_id` for resolution will get "event not found."

**Design gap:** The persistence boundary is `record()`, but the in-memory `DriftEvent` value crosses a task boundary before reaching it. `tick_once` should call `record()` internally before returning `Drifted`, making detection and persistence atomic from the caller's perspective.

---

### F035 — `/storage/.*` Ignore Rule Masks Future Caddy Module Config Written Under `/storage/` [MEDIUM]

**Category:** Assumption violation

**Attack:** The `/storage/.*` rule is intended to cover Caddy's TLS certificate automation paths. But `/storage/` is a generic Caddy key-value namespace. Any present or future Caddy module that stores config-relevant state under `/storage/` (e.g., a rate-limiter storing rules at `/storage/ratelimit/rules`) is silently ignored. Config changes to such paths are never detected as drift.

**Scenario:**
1. Caddy adds a new module storing config at `/storage/ratelimit/rules`.
2. Operator changes rate-limit rules directly in Caddy. Drift should be detected.
3. `is_caddy_managed("/storage/ratelimit/rules")` → true. Entry discarded. `tick_once` returns `Clean`.
4. Trilithon has no drift record for a config change it was responsible for managing.

**Design gap:** The rule must be narrowed to specific Caddy-owned sub-paths (`/storage/acme/`, `/storage/ocsp/`) rather than the entire `/storage/` namespace. The current rule is correct for today's Caddy but is a time bomb for future module additions.

---

### F036 — `defer_for_manual_reconciliation` Has No Owner Notification Path and No TTL [LOW]

**Category:** Observability gaps

**Attack:** `defer_for_manual_reconciliation` produces a no-op mutation. The design specifies no outbound notification beyond the `drift.resolved` tracing event (which records a resolution, not a deferral). There is no TTL, no escalation, no `drift.deferred` tracing event in §12.1. If the operator who deferred does not return to resolve, the event persists indefinitely with no re-alerting (subject to the `last_running_hash` state per F021).

**Design gap:** Deferral assumes a human will observe and act via an unspecified external mechanism. The design should specify a maximum deferral window, a `drift.deferred` tracing event name to be added to §12.1, and behavior on TTL expiry (reset hash → re-detect).

---

## Summary

**Critical:** 3 (F026, F027, F028)
**High:** 4 (F029, F030, F031, F032)
**Medium:** 3 (F033, F034, F035)
**Low:** 1 (F036)

**Top concern:** F026 — the `/storage/.*` ignore rule makes ownership-sentinel removal *invisible* to the drift detector. This is not an edge case: it is a guaranteed, deterministic failure on every ownership-loss event. The drift detector, which is the primary runtime safety mechanism for ADR-0015, provides zero protection against exactly the attack ADR-0015 was designed to prevent.

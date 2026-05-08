# Phase 8 Adversarial Review — Round 1

**Date:** 2026-05-07
**Severity summary:** 2 critical · 6 high · 5 medium · 2 low

---

## Findings

### F001 — Deduplication hash survives resolution, blinds next real drift [CRITICAL]

**Category:** State machine gaps

**Attack:** `mark_resolved` resets `last_running_hash` to `None`, but `adopt_running_state` does not call `mark_resolved` — it only produces a `Mutation`. If the mutation worker applies the adoption but the calling code never invokes `mark_resolved`, the hash stays set to the adopted running-state hash. The next real drift (Caddy config changes again) computes a new running-state hash, so deduplication fires correctly — but if Caddy transiently flaps back to the same hash that was adopted, the audit row is silently suppressed forever. More critically: if `record()` is called before the mutation is actually applied (the design shows `record(event)` in slice 8.6 with no guarantee of ordering relative to the mutation worker), the hash is committed, the mutation fails, and the next tick sees the same drifted hash and silently skips it. The system believes drift is resolved when Caddy is still diverged.

**Scenario:**
1. Drift detected. `running_state_hash = "abc"`.
2. `record(event)` writes audit row and sets `last_running_hash = Some("abc")`.
3. `adopt_running_state` produces a `Mutation`. Mutation worker attempts to apply; storage write fails transiently; mutation is dropped.
4. Next tick: Caddy still returns same config. `running_state_hash = "abc"`. Deduplication fires: `Some("abc") == Some("abc")`, `return Ok(())`. No audit row, no alert.
5. Drift persists indefinitely without any signal.

**Design gap:** The design records the running hash on detection, not on confirmed resolution. There is no feedback path from the mutation worker's success or failure back to `last_running_hash`. Deduplication should reset only on confirmed successful application, not on detection.

---

### F002 — `apply_diff` round-trips through reparse, losing type fidelity on redacted state [CRITICAL]

**Category:** Semantic drift between layers

**Attack:** `apply_diff` mutates `state.canonical_json()` at each pointer and then "reparses." The redacted diff (`redacted_diff_json`) is the form stored in the audit log. If `adopt_running_state` or `reapply_desired_state` is later fed a `DesiredState` reconstructed from the audit row (e.g., during a replay or a dual-pane editor read), the state contains redacted placeholders rather than real values. The design specifies that `RedactedDiff` is a newtype — but `apply_diff` takes `&Diff`, not `&RedactedDiff`, meaning nothing at the type level prevents a redacted diff from being passed to `apply_diff`. If the UI constructs a `Diff` from the stored `redacted_diff_json` and passes it through `apply_diff`, the resulting `DesiredState` will have sentinel/placeholder values written into live Caddy config at apply time.

**Scenario:**
1. Drift detected. Diff contains TLS private key material at `/apps/tls/...`.
2. Redactor replaces key material with `"[REDACTED]"` in `redacted_diff_json`.
3. Audit row stores `redacted_diff_json`.
4. Operator opens dual-pane editor (architecture §7.2). UI reads audit row, reconstructs a `Diff` from `redacted_diff_json`.
5. Operator clicks "re-apply desired state." Code path (incorrectly) feeds reconstructed `Diff` through `apply_diff`.
6. Caddy receives `"[REDACTED]"` as a TLS certificate value. Caddy rejects the config or silently uses an invalid cert.

**Design gap:** `apply_diff` must only accept `Diff` computed from live state, never deserialized from audit rows. The type system does not enforce this. A `LiveDiff` / `AuditDiff` distinction, or making `Diff` non-deserializable, would close this gap.

---

### F003 — Ignore list compiled with `once_cell::Lazy` matches against `JsonPointer` paths but the pointer format is never specified [HIGH]

**Category:** Assumption violation

**Attack:** The ignore-list regexes assume paths like `/apps/tls/automation/policies/abc/managed_certificates`. `JsonPointer` is a type — but the design does not specify whether `JsonPointer::to_string()` (used for matching) produces RFC 6901 encoding (with `~0`/`~1` escapes) or raw decoded form. A Caddy policy name containing a slash (e.g., `my/policy`) would encode to `my~1policy` in RFC 6901 form. The regex `[^/]+` does not match `~1` — it would match `my` and stop, causing the path `/apps/tls/automation/policies/my~1policy/managed_certificates` to fail the ignore list match. That path would then appear as a real diff entry, generating spurious drift events on every tick for a config that is legitimately Caddy-managed.

**Scenario:**
1. Caddy config has a TLS policy named `tenant/prod`.
2. Flattener emits key `/apps/tls/automation/policies/tenant~1prod/managed_certificates/0`.
3. Regex `^/apps/tls/automation/policies/[^/]+/managed_certificates(/.*)?$` does not match (sees `tenant~1prod` as containing `~` but not `/`).
4. Every 60-second tick generates a `config.drift-detected` audit row for a certificate path that is intentionally Caddy-managed.
5. Audit table fills with false positives. Operators become desensitised to drift alerts.

**Design gap:** The design must specify whether `JsonPointer` matching uses RFC 6901-encoded or decoded string form, and the ignore-list regexes must be written for whichever form is chosen. The two must be locked together.

---

### F004 — `try_lock` on apply mutex drops the guard immediately; no re-entry window [HIGH]

**Category:** Data race / interleaving

**Attack:** Slice 8.5 step 1 calls `try_lock` on the apply mutex and, if successful, proceeds with the tick. But the design does not show the lock being *held* across the entire tick — it says "try_lock the apply mutex. If Err, return SkippedApplyInFlight." A `try_lock` that succeeds returns a `MutexGuard`. If the tick code does not hold that guard for its entire duration (i.e., the guard is dropped before step 2), the apply mutex provides no mutual exclusion. An apply operation arriving from the mutation worker during steps 2–6 of the tick would proceed concurrently, applying a new snapshot while the diff engine is comparing against the "latest snapshot" read in step 3.

**Scenario:**
1. Tick starts. `try_lock` succeeds; guard is held.
2. Guard accidentally dropped early. Mutation worker acquires the mutex and begins applying a new snapshot.
3. Tick reads new desired-state snapshot (step 3) that now reflects the in-progress apply.
4. `GET /config/` (step 2) returns the pre-apply Caddy state.
5. Diff shows divergence between the new desired state and the old running state — a false drift.
6. `DriftEvent` is emitted and recorded. Audit log contains a drift event for a config transition that was already in progress.

**Design gap:** The design must explicitly state that the apply mutex guard is held for the full duration of `tick_once` — from `try_lock` through the return of `Drifted` or `Clean`. The pseudocode must make guard lifetime explicit.

---

### F005 — SHA-256 hash of canonical JSON computed before redaction; stored hash does not match stored diff [HIGH]

**Category:** Missing invariant enforcement

**Attack:** Slice 8.5 step 5 computes `running_state_hash = SHA-256(canonical_json)`. Step 6 builds `DriftEvent` with `redacted_diff_json`. The hash is of the *full* running state, but `redacted_diff_json` is a redacted view. An operator looking at the audit row cannot verify that the `redacted_diff_json` was derived from the state producing that hash. More critically, if two different Caddy configurations differ only in redacted fields, the deduplication in slice 8.6 correctly differentiates them (hash covers full JSON) — but the audit row's `redacted_diff_json` presents identical content for both events, making them indistinguishable in the audit UI.

**Scenario:**
1. Drift event A: TLS key rotated to `key-v2`. `running_state_hash = "abc"`. Audit row written.
2. Drift event B: TLS key rotated to `key-v3`. Different hash. New audit row written.
3. Both rows have identical `redacted_diff_json` (both show `[REDACTED]` for the key field).
4. Operator reviewing audit rows cannot distinguish the two events from their stored diffs alone.

**Design gap:** `redaction_sites: u32` is insufficient to reconstruct what was redacted. The audit row must store `running_state_hash` alongside `redacted_diff_json`, and the invariant "hash is of pre-redaction canonical JSON" must be documented and tested.

---

### F006 — `ObjectKind` classifier uses static prefix matching with wildcard `*` that is not glob syntax [HIGH]

**Category:** Boundary condition exploits

**Attack:** Slice 8.3 specifies ObjectKind classification via "static prefix matching" with patterns like `/apps/http/servers/*/routes/*`. The `*` here is wildcard syntax. But the design places this in `core` (no I/O, no external deps) and does not import a glob library. If the implementation uses `str::starts_with` or a hand-rolled prefix check, `*` is a literal character, not a wildcard. A path `/apps/http/servers/prod-1/routes/main` would not match `/apps/http/servers/*/routes/*` via `starts_with`. This would silently classify all `Route` and `Server` entries as `Other`, collapsing all diff counts into the `Other` bucket.

**Scenario:**
1. Caddy config drifts: 5 routes removed, 2 upstreams changed.
2. ObjectKind classifier compares `/apps/http/servers/prod/routes/0` against `/apps/http/servers/*/routes/*`.
3. `starts_with("/apps/http/servers/*/routes/")` returns false (literal `*` not in path).
4. All 5 entries fall through to `Other`.
5. `diff_summary` shows `{ Other: { removed: 5, modified: 2 } }` instead of `{ Route: { removed: 5 }, Upstream: { modified: 2 } }`.
6. Audit row and any downstream alerting keyed on `Route` diffs fires no alert.

**Design gap:** The design must specify the exact matching algorithm for ObjectKind classification — either proper glob matching (e.g., `glob` crate), a path-segment-aware comparator, or explicit prefix strings without wildcards. "Static prefix matching" with `*` notation is ambiguous.

---

### F007 — `reapply_desired_state` re-applies `before_snapshot_id`, which may no longer be the latest desired state [HIGH]

**Category:** State machine gaps

**Attack:** `reapply_desired_state` produces `Mutation::ReapplySnapshot { snapshot_id: event.before_snapshot_id }`. `before_snapshot_id` is the snapshot at drift-detection time. Between detection and resolution, an operator may have applied a new desired-state snapshot. Calling `reapply_desired_state` on a stale event would push an older snapshot to Caddy, silently overwriting any config changes made after the drift was detected.

**Scenario:**
1. T=0: Drift detected against snapshot `S-10`. `DriftEvent` records `before_snapshot_id = S-10`.
2. T=30s: Operator applies a new config, creating snapshot `S-11`. Current desired state is now `S-11`.
3. T=60s: Operator resolves drift event by choosing "re-apply desired state." Code calls `reapply_desired_state(event, ...)`, which emits `Mutation::ReapplySnapshot { snapshot_id: S-10 }`.
4. Mutation worker applies `S-10` to Caddy, silently rolling back the change made at T=30s.
5. Snapshot `S-11` is now orphaned; the running state has regressed.

**Design gap:** `reapply_desired_state` must verify that `event.before_snapshot_id` matches the *current* desired-state snapshot ID before producing the mutation, and must return a `ResolveError` if they diverge. Alternatively the API should accept the *current* desired state rather than using the event's stale snapshot reference.

---

### F008 — Drift loop `tick_once` has no timeout on `GET /config/` [HIGH]

**Category:** Backpressure and resource exhaustion

**Attack:** Slice 8.5 step 2 issues `GET /config/` from Caddy with no specified timeout. If Caddy is under load or in a drain window, the HTTP call may hang. Because the tick holds the apply mutex (per the design intent), a hanging `GET /config/` will block the apply mutex for the duration of the hang. The mutation worker cannot acquire the mutex to apply any changes. The entire apply pipeline stalls.

**Scenario:**
1. Caddy undergoes a hot-reload with a 90-second drain window.
2. `tick_once` acquires apply mutex and issues `GET /config/`.
3. Request hangs for 90 seconds.
4. Apply mutex is held for 90 seconds. Queued mutations cannot be applied.
5. A second tick fires, tries `try_lock`, gets `Err`, returns `SkippedApplyInFlight` — misleadingly labelled since the "apply in flight" is actually a stalled drift check.

**Design gap:** The design must specify a timeout for `GET /config/` in `tick_once` (suggested: shorter than the minimum tick interval of 10s). On timeout, release the mutex and return a new `TickOutcome::FetchTimeout` variant.

---

### F009 — Flattener emits only scalar leaves; arrays of objects produce phantom modifications [MEDIUM]

**Category:** Boundary condition exploits

**Attack:** For an array of objects like `routes: [A, B, C]`, the flattener produces paths by index. If a route is inserted at index 0, every subsequent route shifts by 1. The diff engine sees index 0 as `Modified` (A→X), index 1 as `Modified` (B→A), index 2 as `Modified` (C→B), index 3 as `Added` (C). This produces inflated `DiffCounts` and a misleading audit trail, even though only one route was inserted.

**Scenario:**
1. Desired state: routes `[A, B, C]`. Caddy running state: routes `[X, A, B, C]` (X prepended).
2. Diff shows: 3 modified + 1 added instead of 1 added.
3. `DiffCounts` shows `{ Route: { added: 1, modified: 3 } }`.
4. Operator investigates 3 modified routes; none are actually changed.

**Design gap:** The design should document this known distortion explicitly. Operators must be warned that route-ordering changes produce over-counted diffs. This is not a blocker but must be in the design notes.

---

### F010 — `DriftEvent.detected_at` ordering breaks under NTP corrections or VM migrations [MEDIUM]

**Category:** Assumption violation

**Attack:** `detected_at: i64` stores unix seconds. If the system clock steps backward (NTP correction, VM migration), two sequential `DriftEvent`s can have non-monotonic timestamps. Queries ordered by `detected_at` return rows out of sequence. The `correlation_id: Ulid` is monotonically ordered by generation time and would correctly sequence events — but if the audit viewer sorts by `detected_at`, history appears scrambled.

**Scenario:**
1. Drift detected at `T=1000`. Row written.
2. NTP steps clock back to `T=880`.
3. Drift detected again at `T=882`. Row written.
4. Query `ORDER BY detected_at` returns second event before first.

**Design gap:** Document that `detected_at` is "wall-clock for display only." Specify that `correlation_id` (ULID) or insert-order rowid is the canonical ordering key for the audit viewer.

---

### F011 — `record()` holds `last_running_hash` lock across an async storage write [MEDIUM]

**Category:** Data race / interleaving

**Attack:** The `record()` algorithm holds `last_running_hash` lock across `storage.record_drift_event()` (an async I/O call — SQLite write). Holding a `tokio::sync::Mutex` across an `await` is valid in Tokio, but it means the lock is held for the full duration of the storage write. Any concurrent call to `mark_resolved` (from a user-facing API handler) blocks for the duration of that write.

**Scenario:**
1. Tick fires, `record(event)` acquires lock and begins SQLite write (50ms).
2. Operator triggers resolution. Handler calls `mark_resolved`. Blocks for 50ms.
3. No data corruption, but UI response latency is unexpectedly coupled to storage write latency.

**Design gap:** Read the hash under lock, release lock, perform storage write, re-acquire lock only to update hash. This eliminates the coupling between storage latency and API response latency.

---

### F012 — `structural_diff` may or may not include `unknown_extensions` paths; design is silent [MEDIUM]

**Category:** Semantic drift between layers

**Attack:** Slice 8.3 adds `unknown_extensions: BTreeMap<JsonPointer, serde_json::Value>` to `DesiredState`. Slice 8.1 diffs via `flatten(state.canonical_json())`. If `canonical_json()` excludes `unknown_extensions` (stored separately to avoid re-serialization conflicts), drift in extension paths will be invisible. A third-party Caddy plugin config change would never be detected.

**Scenario:**
1. Caddy config includes `/apps/custom_plugin/setting` captured in `unknown_extensions`.
2. `canonical_json()` excludes these paths.
3. Plugin config drifts (setting changed externally).
4. `structural_diff` sees no change. Drift undetected.

**Design gap:** The design must explicitly specify whether `canonical_json()` includes `unknown_extensions` paths, and document the consequence if excluded.

---

### F013 — Skipped ticks are silent in the audit log [MEDIUM]

**Category:** Observability gaps

**Attack:** `SkippedApplyInFlight` produces no audit row. If the apply mutex is held for an extended period, every subsequent tick returns silently. An operator watching the audit log cannot distinguish "system was clean for 10 minutes" from "drift checks were suppressed for 10 minutes." A config change during the suppression window would not be detected until the mutex releases.

**Scenario:**
1. Apply mutex stuck for 10 minutes.
2. Ten drift-check ticks return `SkippedApplyInFlight`. No audit rows.
3. Caddy config changed externally during minute 5.
4. Drift detected at minute 10. Audit log shows no evidence checks were suppressed during minutes 1–10.

**Design gap:** A `config.drift-check-skipped` audit kind (or at minimum a structured log event added to §12.1) should be emitted for skipped ticks. The open question in the TODO acknowledges this but does not resolve it.

---

### F014 — `redact_diff` relies solely on schema registry completeness for secret detection [LOW]

**Category:** Trust boundary violations

**Attack:** `redact_diff` relies on `SchemaRegistry` to identify secret-bearing paths. If Caddy returns a config with a secret at an unregistered path (new Caddy version, third-party plugin), it passes through unredacted into `redacted_diff_json` and the audit row, violating hazard H10.

**Scenario:**
1. Caddy 2.9 adds `upstream.basic_auth.password` (not in schema registry built for 2.8).
2. Drift involves upstream auth config. `redact_diff` does not redact the password.
3. Plaintext password written to audit row.

**Design gap:** Specify a "deny-by-default" redaction posture for unknown paths matching heuristic patterns (`password`, `secret`, `key`, `token`, `cert` as path-segment substrings), supplementing the schema registry.

---

### F015 — Deferred events have no TTL; drift can persist indefinitely without re-alerting [LOW]

**Category:** Missing invariant enforcement

**Attack:** `defer_for_manual_reconciliation` sets `last_running_hash` via `record()` and never resets it unless `mark_resolved` is called. If an operator defers and then closes the browser without resolving, `last_running_hash` remains set. Every subsequent tick with the same drifted config is silently deduped. No further audit rows or alerts are emitted until someone explicitly calls `mark_resolved` with the correct `correlation_id`.

**Scenario:**
1. Drift detected. Operator clicks "defer." `last_running_hash = Some("abc")`.
2. Operator closes browser without resolving.
3. Caddy config remains drifted. Every subsequent tick: silent deduplication.
4. Drift persists for days without signal.

**Design gap:** Specify a maximum deferral TTL (e.g., 24h), after which `last_running_hash` is reset and a new `config.drift-detected` row is written, or define a `config.drift-deferred-timeout` audit kind.

---

## Summary

**Critical:** 2 · **High:** 6 · **Medium:** 5 · **Low:** 2

**Top concern:** F001 — the deduplication hash is committed on detection rather than on confirmed resolution. A single transient mutation failure silently suppresses all future drift alerts for the same config state, creating a persistent blind spot with no observable signal.

**Recommended action before proceeding:** Address criticals first. F001 requires a feedback path from the mutation worker's success back to `last_running_hash` before any code is written. F002 requires a type-level barrier preventing deserialized audit diffs from entering `apply_diff`. Both are fundamental to the correctness of the resolution flow and will be expensive to retrofit after slices 8.4–8.6 are implemented.

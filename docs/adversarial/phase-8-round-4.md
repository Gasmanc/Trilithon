# Phase 8 Adversarial Review — Round 4

**Date:** 2026-05-08
**Severity summary:** 3 critical · 4 high · 3 medium · 1 low

---

## New Findings (Round 4)

### F037 — `tokio::time::interval(Duration::from_secs(0))` panics on zero interval [CRITICAL]

**Category:** Boundary condition exploits

**Attack:** An operator sets `drift_check_interval_seconds = 0` (plausible misconfiguration). `DriftDetectorConfig` has no `validate()` method and no `TryFrom` rejecting out-of-range values. `run()` calls `tokio::time::interval(Duration::from_secs(0))`, which panics with "period must be non-zero."

**Scenario:**
1. Operator copies a template config with `drift_check_interval_seconds = 0` as a placeholder.
2. Daemon starts. `run()` panics. The task is dropped silently.
3. The drift loop never runs. No error is logged to the audit trail. No sentinel check ever fires on drift. No `config.drift-detected` rows are ever written.
4. The daemon appears healthy (HTTP endpoint responds, other tasks run), but drift detection is completely dead.

**Design gap:** The architecture constrains the interval to [10, 3600] but no layer enforces this. `DriftDetectorConfig` has no validation. CLI `clap` validation applies only to flags, not config-file values.

---

### F038 — Audit write succeeds, drift-event-table write fails: `mark_resolved` targets a non-existent row [CRITICAL]

**Category:** Partial failure atomicity

**Attack:** `record()` writes to `AuditWriter` (step 3) then calls `storage.record_drift_event(event)` (step 4) as two independent writes. If the audit write succeeds but the drift-event table write fails (disk full, schema mismatch, unique constraint violation from a restart race), an audit row exists with `correlation_id = X` but no corresponding `DriftEventRow`.

**Scenario:**
1. Audit row written: `config.drift-detected`, `correlation_id = X`.
2. `storage.record_drift_event` fails. `record()` returns `Err`. `last_running_hash` not updated.
3. Phase 9 calls `storage.mark_resolved(X, resolution)`. No `DriftEventRow` found.
4. Either returns `Ok(0 rows updated)` (silent no-op) or `Err`. In the no-op case, `last_running_hash` is never reset to `None`. All subsequent ticks for the same drift hash deduplicate silently. The operator applied a resolution; the system behaves as if drift persists forever.

**Design gap:** The two writes have no transactional envelope. There is no compensating write on failure, and no specification of what `record()` returns to the caller when the second write fails.

---

### F039 — Raw Caddy JSON parsed into `DesiredState` silently drops unmodeled fields; diff reports false-clean for any Caddy feature Trilithon doesn't model [CRITICAL]

**Category:** Semantic drift between layers

**Attack:** `tick_once` step 2 parses raw `GET /config/` JSON into `DesiredState`. The struct models only the Trilithon-understood subset of Caddy configuration. Any Caddy field Trilithon does not know about is silently dropped (if `deny_unknown_fields` is absent) or causes a parse error (if present). The stored desired state is also a `DesiredState` struct. The diff engine compares two projections through the same partial lens — field-level drift for anything outside the model is invisible.

**Scenario A (silent false-clean):** An operator manually adds a Caddy `grace_period` field at the server level. Trilithon does not model this. `GET /config/` returns it; `DesiredState` parse drops it. Both operands of `structural_diff` are identical. `tick_once` returns `Clean`. Drift is never detected.

**Scenario B (parse error storm):** `deny_unknown_fields` is set. A Caddy plugin adds fields. Every `tick_once` step 2 fails to parse. The design specifies no error path for parse failure — the tick returns some unspecified error with no audit row.

**Design gap:** The design conflates "parse Caddy JSON to DesiredState" with "get a comparable snapshot of running config." These are not the same operation. The diff is only meaningful if both operands represent the same universe of fields. The design does not specify whether `DesiredState` is a closed or open model when used as a diff operand, and the `unknown_extensions` field introduced in slice 8.3 is insufficient if it is not included in the flat map used by `structural_diff`.

---

### F040 — Shutdown fires mid-tick: in-flight SQLite write abandoned without commit guarantee [HIGH]

**Category:** Partial failure atomicity

**Attack:** If `run()` uses `tokio::select!` with `shutdown.changed()` racing the tick future, the tick future can be cancelled mid-await inside `record()`'s SQLite write. `sqlx` uses implicit per-statement transactions. A dropped Future does not roll back an implicit transaction — it abandons the connection.

**Scenario:**
1. `record()` is mid-flight: `AuditWriter` write done, `storage.record_drift_event` awaiting.
2. SIGTERM fires. `run()` selects `shutdown.changed()`. Tick future is dropped.
3. SQLite write not committed. `AuditWriter` row exists; `DriftEventRow` does not — same split-write as F038, but caused by cancellation rather than error.
4. `last_running_hash` not updated. Same permanent dedup-suppression consequence.

**Design gap:** The design says "loop terminates cleanly within one tick on shutdown" but does not define "cleanly." Async cancellation of `sqlx` futures is unsafe unless wrapped in explicit `BEGIN`/`COMMIT`. The shutdown contract must specify whether an in-flight tick is completed or abandoned.

---

### F041 — `Storage::latest_desired_state()` ordering semantics unspecified; wall-clock ordering returns stale snapshot after rollback [HIGH]

**Category:** Assumption violation

**Attack:** Two snapshots exist for the same `caddy_instance_id`: `config_version = 5, created_at_ms = T` and `config_version = 4, created_at_ms = T+1` (version 4 was re-snapshotted after a rollback, receiving a later wall-clock timestamp than version 5). If `latest_desired_state()` orders by `created_at_ms DESC`, it returns version 4 as the "current" desired state.

**Scenario:**
1. `latest_desired_state()` returns snapshot at `config_version = 4`.
2. Caddy is running config at `config_version = 5` (the pre-rollback state).
3. Every `tick_once`: diff between v4 and v5 appears as drift. Detection fires continuously.
4. Dedup guard suppresses repeat rows for the same hash, but any change in the running config resets the hash and a new flood begins.

**Design gap:** The design calls `storage.latest_desired_state()` without specifying the sort key. `config_version` is the correct monotonic ordering criterion, but the design does not mandate it. Any implementation using wall-clock time is vulnerable to rollback timestamp inversions and NTP corrections.

---

### F042 — `DiffEngine` trait does not contractually require lexicographic ordering; non-deterministic `Diff` breaks `apply_diff` idempotency [HIGH]

**Category:** Missing invariant enforcement

**Attack:** The ordering requirement ("entries ordered by `JsonPointer` lexicographically") is an acceptance criterion for `DefaultDiffEngine`, not a type-level contract on the `Diff` struct or the trait. A future implementation (or test double) that returns entries in insertion order is a valid implementation of the trait.

**Scenario:**
1. A `Diff` has entries modifying a parent path at index 0 and a child path at index 1 (lexicographic order: child before parent).
2. A non-lexicographic implementation returns parent at index 0, child at index 1.
3. `apply_diff` walks entries in order. Applies parent modification first (mutating the parent node). Then applies child modification — but the parent's structure has changed. `apply_diff` returns `IncompatibleShape`.
4. `apply_diff` is not idempotent across `DiffEngine` implementations. The same logical diff succeeds or fails depending on which implementation produced it.

**Design gap:** `Diff::entries` is a `Vec<DiffEntry>`, not a `BTreeMap<JsonPointer, DiffEntry>`. Ordering is enforced only by documentation, not structure.

---

### F043 — `correlation_id` ULID timestamp and `detected_at: i64` derived from different clock reads; audit trail has two conflicting event times [MEDIUM]

**Category:** Observability gaps

**Attack:** `Ulid::new()` uses the system monotonic clock (milliseconds). `detected_at: i64` is set from `std::time::SystemTime::now()` (wall clock, unix seconds). Both nominally represent "when drift was detected" but are derived independently.

**Scenario:**
1. Under load, the two calls span a second boundary: ULID timestamp = T ms, `detected_at = T/1000 + 1`.
2. An auditor correlating events by timestamp finds the ULID timestamp predates `detected_at` — discrepancy looks like log tampering.
3. If NTP steps the clock backward between the two calls, `detected_at` is earlier than the ULID timestamp by an arbitrary amount.
4. `detected_at: i64` is unix seconds (no sub-second resolution); ULIDs have millisecond precision. They are not comparable at sub-second granularity — a systematic audit inconsistency.

**Design gap:** The design treats `correlation_id` and `detected_at` as independent fields without specifying which is authoritative or requiring derivation from the same clock read. `detected_at` should be derived from `ulid.timestamp_ms() / 1000` to guarantee consistency.

---

### F044 — `RedactedDiff` newtype constructor is not restricted; alternate `DiffEngine` implementations can bypass schema-aware redaction [HIGH]

**Category:** Trust boundary violations

**Attack:** `DiffEngine::redact_diff` returns `RedactedDiff` — the sole gate between unredacted diff content and `AuditWriter`. `RedactedDiff` is a newtype. If its constructor is `pub`, any `DiffEngine` implementation can construct it directly (e.g., `RedactedDiff(original_diff)`) without invoking `SchemaRegistry::redact`.

**Scenario:**
1. A future `ExplainerDiffEngine` wraps `DefaultDiffEngine` to add natural-language annotations.
2. Its `redact_diff` copies the `DefaultDiffEngine` output but the annotation strings echo original field values.
3. Constructs `RedactedDiff(annotated_diff)`. `AuditWriter` accepts it. Secret values in annotations are written to the audit log.
4. ADR-0009 makes audit rows immutable. The secrets are permanently in the audit log.

**Design gap:** `RedactedDiff`'s constructor must be `pub(crate)` or accessible only through a `SchemaRegistry`-verified path. The design does not restrict the constructor visibility, allowing any implementation to bypass redaction via direct construction.

---

### F045 — `drift_check_interval_seconds = u32::MAX` silently disables drift detection for ~136 years [MEDIUM]

**Category:** Boundary condition exploits

**Attack:** An operator sets a very large interval (e.g., `u32::MAX = 4294967295` seconds). Unlike F037 (zero → panic), this is silent. `tokio::time::interval(Duration::from_secs(4294967295))` creates a valid interval that sleeps for ~136 years. The drift loop starts, calls `tick()`, and never fires.

**Scenario:**
1. Operator sets `drift_check_interval_seconds = 4294967295`.
2. Daemon starts. No panic. Health endpoint responds normally.
3. Drift loop is technically running but its first tick fires in 2162. No drift is ever detected.
4. No error is logged. No audit row. The system appears operational.

**Design gap:** Same root cause as F037. The same `DriftDetectorConfig::validate()` fix covers both.

---

### F046 — Dedup guard non-atomic with SQLite write; concurrent callers can both pass the guard and write duplicate rows [MEDIUM]

**Category:** Data race / interleaving

**Attack:** If `record()` releases the `last_running_hash` mutex before the SQLite write completes (which the design implies — step 4 is `async`, and Tokio mutexes cannot be held across a `.await` in safe code without explicit scoping), two concurrent callers with the same `running_state_hash` can both read `None`, both pass the dedup check, and both issue SQLite writes.

**Scenario:**
1. Two tasks both call `record(event)` where `event.running_state_hash = "abc"`.
2. Task A: locks guard, reads `None`, releases guard (before write).
3. Task B: locks guard, reads `None` (A hasn't updated it yet), releases guard.
4. Both tasks issue `storage.record_drift_event`. Both writes succeed (or second fails with unique constraint violation, which is unhandled).
5. Two identical `config.drift-detected` rows in the audit log for the same drift event.

**Design gap:** The check-then-act on `last_running_hash` is not atomic with the SQLite write. Either the mutex must be held across the write, or a unique constraint on `correlation_id` in the drift events table must be the backstop (with explicit handling of the constraint violation).

---

### F047 — `SkippedApplyInFlight` detection gap unbounded; no post-apply wake-up [LOW]

**Category:** Backpressure and resource exhaustion

**Attack:** The apply mutex is held for the duration of a slow apply (e.g., large config push, 90 seconds). The detector skips one full 60-second tick. If the apply takes longer than the interval, multiple ticks are skipped. The architecture §13 guarantee of "detect within one interval" is silently violated. No mechanism wakes the detector when the mutex is released — it waits for the next scheduled tick.

**Scenario:**
1. Apply takes 90 seconds. Two ticks are skipped.
2. Caddy config is changed externally during the apply window.
3. Detection delay: 90s (apply duration) + up to 60s (next scheduled tick) = up to 150 seconds before drift is reported.
4. In a security incident, this is the window an attacker has before Trilithon notices.

**Design gap:** The design does not specify a post-apply wake-up mechanism. A `watch` channel signalled by the mutation worker on apply completion, causing the detector to immediately check on mutex release, would bound the detection gap to `apply_duration + epsilon`.

---

## Summary

**Critical:** 3 (F037, F038, F039)
**High:** 4 (F040, F041, F042, F044)
**Medium:** 3 (F043, F045, F046)
**Low:** 1 (F047)

**Top concern:** F039 — the schema mismatch between raw Caddy JSON and the Trilithon `DesiredState` model means drift detection is blind to any Caddy field Trilithon does not model. This is the default behavior for any installation using plugins, custom modules, or fields added in Caddy versions released after the struct was last updated. The system reports `Clean` for real drift with no indication that coverage is partial.

# Phase 8 Adversarial Review — Round 5

**Date:** 2026-05-08
**Severity summary:** 4 critical · 6 high · 3 medium · 1 low

---

## New Findings (Round 5)

### F048 — `drift_events` table does not exist in §6 schema [CRITICAL]

**Category:** Missing invariant enforcement

**Attack:** Phase 8 calls `storage.record_drift_event(event)` and `storage.latest_drift_event(instance_id)`. Architecture §6 defines the complete V1 schema — 15 tables — with no `drift_events` table. Three incompatible implementations are possible: (a) a new undeclared table + migration, (b) writes into `audit_log.notes` making the second write in `record()` redundant, (c) a no-op stub. All three cause failures.

**Scenario:**
- Path (c): `record_drift_event` succeeds silently. Phase 9's `mark_resolved` calls `latest_drift_event` → returns `None`. Every drift event is permanently unresolvable. The drift loop can never close.
- Path (a): An undeclared migration ships to production without going through the schema-review checklist in architecture §14, bypassing checksum verification.
- Path (b): The structured drift data is in `audit_log.notes` as an opaque string; `latest_drift_event` must parse it back out, creating an implicit schema inside a TEXT column.

**Design gap:** The design invokes trait methods (`record_drift_event`, `latest_drift_event`) that reference storage that does not exist in the authoritative schema. The implementation path is ambiguous in three incompatible directions, each of which causes a different category of failure.

---

### F049 — `CaddyClient::get_running_config()` does not exist in the trait [CRITICAL]

**Category:** Composition failures

**Attack:** Slice 8.5 step 2 calls `self.client.get_running_config().await?`. The `CaddyClient` trait in `trait-signatures.md` defines five methods: `load_config`, `patch_config`, `get_config_at_path`, `check_ownership_sentinel`, `write_ownership_sentinel`. No `get_running_config` method exists.

**Scenario:**
1. Implementer adds `get_running_config` to the concrete `CaddyHttpClient` without adding it to the trait.
2. `DriftDetector` is generic over `Arc<dyn CaddyClient>`. It cannot call `get_running_config` — the trait doesn't expose it.
3. Implementer either hardcodes `CaddyHttpClient` (breaking testability and the trait abstraction), or adds the method to the trait with an ad-hoc signature that differs from what Phase 8 assumed.
4. All `tick_once` unit tests using a mock `CaddyClient` cannot compile.

**Design gap:** Phase 8 introduces a new required method on a shared trait without specifying its signature, return type (`serde_json::Value`? typed struct?), error mapping, or relationship to `GET /config/`. An undeclared trait extension forces implicit design decisions downstream.

---

### F050 — `run(self)` consumes the detector; panic silently kills drift detection with no restart path [CRITICAL]

**Category:** Cascade construction

**Attack:** `DriftDetector::run` takes `self` by value. If the task panics (e.g., `canonical_json` panics on a non-finite float in the running config, or any `unwrap()` in a transitive dep), the struct is dropped, `last_running_hash` is lost, and the task exits. `run` returns `()` — the caller cannot observe failure. No audit row is written. No supervisor or restart path is specified.

**Scenario:**
1. Caddy config contains a JSON number that triggers a panic in `canonical_json` serialization.
2. The `tick_once` future panics. The spawned task exits.
3. `run()` returns `()`. The daemon's main loop (which spawned the task) either observes a panicked `JoinHandle` (if it awaits it) or never notices (if it's fire-and-forget).
4. Drift detection is dead. No `config.drift-detected` rows are ever written again. No alert fires.

**Design gap:** `run() -> ()` removes the caller's ability to observe task failure. `self` consumption removes the ability to restart. The design specifies no panic boundary, no audit row on unexpected exit, and no supervision strategy.

---

### F051 — `redacted_diff_json` is unbounded TEXT in audit_log; large config diffs violate the §13 storage budget [CRITICAL]

**Category:** Backpressure and resource exhaustion

**Attack:** `audit_log.notes` (or a TEXT column holding `redacted_diff_json`) has no size constraint. A full structural diff of a 1000-route config can be megabytes. Architecture §13 budgets ~50 MiB per million audit rows at 1 KiB average. A single large drift event blows this budget by three orders of magnitude.

**Scenario:**
1. A deployment adds 800 routes simultaneously. `tick_once` produces a ~12 MB `redacted_diff_json`.
2. `AuditWriter` inserts a 12 MB TEXT value into `audit_log`. SQLite loads ~3000 pages into the page cache.
3. All subsequent WAL writes are slower. The audit log row for this single event is 12,000× the average budget.
4. Additionally: `serde_json::to_string` allocates the 12 MB string in memory; the `DriftEvent` struct holds it; `record_drift_event` serializes it again (F061). Peak allocation per tick: ~36 MB for this event alone.

**Design gap:** No cap on `redacted_diff_json` size is specified. The design does not truncate, paginate, or store large diffs as external references. The §13 storage budget is violated by a realistic deployment scenario.

---

### F052 — `DiffError::IncompatibleShape` is unreachable from `structural_diff`; shared enum conflates two distinct error origins [HIGH]

**Category:** Semantic drift between layers

**Attack:** The scalar-leaf flattening algorithm compares leaves by equality — two values at the same path with different types (e.g., `42` vs `"hello"`) are classified as `Modified`, never as `IncompatibleShape`. `IncompatibleShape` requires navigating a pointer segment into a scalar value, which only `apply_diff` can trigger (when it tries to descend into a node that isn't an object/array). The error enum is shared between `structural_diff` and `apply_diff`.

**Scenario:**
1. `tick_once` propagates `TickError::Diff(DiffError::IncompatibleShape { path, .. })`.
2. This can only have come from `apply_diff` (in a resolution path), not `structural_diff`.
3. An operator reading logs sees "structural diff computation failed with IncompatibleShape at /apps/http/servers/srv0/timeout" — they diagnose a corrupt input when the actual cause is a bug in the diff algorithm generating an unapplicable pointer.
4. `IncompatibleShape` from `structural_diff` and from `apply_diff` require different remediation and are indistinguishable from the error value alone.

**Design gap:** `DiffError` conflates two operations with different failure modes. `IncompatibleShape` is dead code for `structural_diff` and live code for `apply_diff`. The shared enum erases which operation failed.

---

### F053 — `with_correlation_span` is a Phase 6 artifact not in Phase 8's declared dependencies [HIGH]

**Category:** Composition failures

**Attack:** Slice 8.5 uses `with_correlation_span(Ulid::new(), "system", "drift-detector", self.tick_once())`. This is tracing-correlation infrastructure from Phase 6. Phase 8 declares dependencies on Phase 5 and Phase 7 only.

**Scenario:**
1. Developer starts Phase 8 implementation before Phase 6 has landed in the branch.
2. `with_correlation_span` is not in scope. Developer implements a local substitute with different field names (`span_id` instead of `correlation_id`).
3. Phase 6 lands. Two correlation-span implementations coexist.
4. Trace queries that join drift-detection spans on `correlation_id` return no results for spans created before the Phase 6 landing. Audit correlation is broken for the entire pre-landing period.

**Design gap:** Phase 8 uses a Phase 6 artifact without declaring Phase 6 as a dependency. Undeclared cross-phase coupling causes divergent implementations when phases ship non-serially.

---

### F054 — `TickError::CaddyFetch(String)` erases structured error type; 404 and 503 produce identical log entries [HIGH]

**Category:** Observability gaps

**Attack:** The typed `CaddyClient` error — which carries HTTP status and body excerpt — is converted to `String` at the `TickError` boundary. Three distinct failure modes become indistinguishable: 404 (Caddy has no loaded config), 503 (Caddy unreachable), 500 (Caddy internal error).

**Scenario:**
1. Caddy is freshly restarted with no config. `get_running_config` returns `Err(CaddyHttp { status: 404 })`.
2. `tick_once` returns `Err(TickError::CaddyFetch("404 Not Found"))`.
3. `run()` logs a warning and continues ticking every 60 seconds.
4. The correct behavior for 404 is `Clean` (no config to diff against — this is a bootstrap state, not drift).
5. Instead, 60 warning logs per hour fire until Caddy loads a config. No code path can distinguish "Caddy empty" from "Caddy down."

**Design gap:** `TickError::CaddyFetch(String)` discards the structured error type needed to make operational decisions. The enum should preserve the HTTP status or transport kind so `run()` can route to `SkippedNoCaddyConfig` vs `SkippedUnreachable` vs `Err`.

---

### F055 — Audit write and drift-event write are not transactional; split failure leaves audit trail permanently inconsistent [HIGH]

**Category:** Partial failure atomicity

**Attack:** `record()` performs two independent writes: `AuditWriter` (step 3) then `storage.record_drift_event` (step 4). If step 3 succeeds and step 4 fails, an audit row exists with no matching drift event row. Phase 9 calls `latest_drift_event` for the unresolved event — returns `None`. `mark_resolved` cannot close the loop.

**Scenario:**
1. `config.drift-detected` audit row written with `correlation_id = X`.
2. `record_drift_event` fails (transient SQLite timeout). `record()` returns `Err`.
3. `last_running_hash` never updated (step 5 not reached). Next tick: dedup passes. New row attempted — but the first audit row now has a sibling. Two audit rows for the same drift, neither resolvable.
4. Operators see two `config.drift-detected` rows for the same event, neither with a corresponding `config.drift-resolved`. The audit trail is permanently split.

**Design gap:** The two writes have no transactional envelope. ADR-0009 requires audit rows to be meaningful and matched; a split write violates this by creating audit rows that can never be resolved.

---

### F056 — `latest_desired_state()` read races with concurrent adoption write; produces false-positive drift [HIGH]

**Category:** Data race / interleaving

**Attack:** Slice 8.5 step 3 reads `storage.latest_desired_state()` while Phase 7 may concurrently write a new adoption snapshot. There is no snapshot isolation specified between these two operations.

**Scenario:**
1. `tick_once` reads `latest_desired_state` → snapshot S1 (`config_version = 5`).
2. Concurrently, Phase 7 adoption completes and writes snapshot S2 (`config_version = 6`, matching Caddy's current state exactly).
3. `structural_diff(&S1, &running)` is non-empty (S1 ≠ running).
4. `tick_once` returns `Drifted`. Audit row written: `config.drift-detected`.
5. Next tick: `latest_desired_state` returns S2. Diff is empty. `tick_once` returns `Clean`. No `config.drift-resolved` row written.
6. The audit log shows a permanently open `config.drift-detected` with no resolution. The false-positive entry is immutable (ADR-0009). An auditor cannot distinguish it from a real unresolved drift.

**Design gap:** No read-your-own-writes guarantee or snapshot isolation is specified for `latest_desired_state` relative to concurrent adoption writes.

---

### F057 — `get_running_config` has no specified timeout; hanging call holds apply mutex indefinitely [HIGH]

**Category:** Backpressure and resource exhaustion

**Attack:** Step 2 of `tick_once` calls `self.client.get_running_config().await?` with no timeout. While this call hangs, the apply mutex guard from step 1 is held. Phase 7 adoption attempts that need the mutex block indefinitely.

**Scenario:**
1. Caddy accepts TCP connections but is internally deadlocked (goroutine stuck on a lock).
2. `get_running_config` hangs indefinitely.
3. Apply mutex held for the duration.
4. Phase 7 mutation worker calls `apply_mutex.lock().await` — blocks forever.
5. All mutations (including user-submitted ones via the web UI) queue up but never execute.
6. Users see "mutation submitted" but never "mutation applied." The daemon appears to be processing but all mutations stall.

**Design gap:** F008 (round 1) noted no timeout on `GET /config/`. F057 is the same gap but specifically on the newly required `get_running_config` method which has no spec at all (per F049). Until F049 is resolved, F057 cannot be fixed either — the timeout must be specified as part of the method signature.

---

### F058 — Running config → `DesiredState` conversion unspecified; Trilithon metadata fields appear as spurious diff entries [MEDIUM]

**Category:** Semantic drift between layers

**Attack:** Step 2 says `get_running_config() → convert to DesiredState`. `DesiredState` has Trilithon-specific fields (`config_version`, `instance_id`, `created_at`, `trilithon_version`) not present in Caddy's running JSON. The conversion must assign values for these fields; whatever sentinel values are chosen will differ from the storage snapshot's values and appear in the diff.

**Scenario:**
1. Desired state from storage has `config_version = 42`.
2. Running-state-as-`DesiredState` has `config_version = 0` (default).
3. `structural_diff` flattens both. The `config_version` path differs.
4. Every tick produces a `Modified` entry for `config_version`. The diff is never clean.
5. The false `Modified` entry inflates `DiffCounts` and pollutes `redacted_diff_json` in every audit row.

**Design gap:** The conversion is unspecified. `DesiredState` must be stripped of Trilithon metadata fields before being used as a diff operand — or a separate `RunningStateView` type that excludes metadata must be introduced.

---

### F059 — `last_running_hash` has no instance_id dimension; multi-instance deployments cross-contaminate dedup [MEDIUM]

**Category:** State machine gaps

**Attack:** `last_running_hash: Mutex<Option<String>>` is a single value with no `caddy_instance_id` key. If the design is 1:1 (`DriftDetector` per Caddy instance), two separate detectors run in the same process and share an `AuditWriter` but each holds its own hash. If the design is 1:N (one `DriftDetector` for multiple instances, iterated), the single hash is overwritten on every tick with whatever the last instance's hash was, suppressing re-detection for earlier instances.

**Design gap:** The design specifies `instance_id: "local"` in `DriftDetectorConfig` for V1, but does not specify the cardinality or explicitly close the door on 1:N usage. The in-memory hash has no instance dimension, making any N>1 usage incorrect by construction.

---

### F060 — `run` shutdown protocol on `watch::Receiver<bool>` unspecified; incorrect value convention prevents clean exit [MEDIUM]

**Category:** Missing invariant enforcement

**Attack:** `run(self, shutdown: tokio::sync::watch::Receiver<bool>)` does not specify which value (`true` or `false`) signals shutdown, or how channel-close (sender dropped) is handled. `while !*shutdown.borrow()` exits on `true` but never exits on channel-close. `shutdown.changed().await` exits on any value change (including `false` → `false` no-op on re-broadcast) and returns `Err` on channel-close — but `Err` may be treated as an error rather than a clean shutdown signal.

**Design gap:** The shutdown protocol is undefined. An implementation that uses `while !*shutdown.borrow()` runs forever after the daemon's shutdown sender is dropped (graceful shutdown without sending `true`). This leaves the drift loop running after the HTTP server, mutation worker, and audit writer have all shut down — writing to a closed `AuditWriter` on the next tick.

---

### F061 — `DriftEvent` stores diff as pre-serialized `String`; double-serialization triples peak memory allocation [LOW]

**Category:** Backpressure and resource exhaustion

**Attack:** `DriftEvent.redacted_diff_json: String` holds the already-serialized diff. `record_drift_event(event: DriftEvent)` serializes `DriftEvent` to insert into storage — serializing the `String` again as an escaped JSON string. For a 5 MB diff: the original `String` (5 MB) + the serialized `DriftEvent` bytes containing the escaped diff (6 MB) + the SQLite write buffer (6 MB) = 17 MB peak allocation per `record()` call.

**Design gap:** Storing a pre-serialized `String` inside a struct that will itself be serialized forces double-serialization and triple allocation. `DriftEvent` should store the diff as a typed `RedactedDiff` and serialize once at the storage boundary.

---

## Summary

**Critical:** 4 (F048, F049, F050, F051)
**High:** 6 (F052, F053, F054, F055, F056, F057)
**Medium:** 3 (F058, F059, F060)
**Low:** 1 (F061)

**Top concern:** F049 and F048 together make Phase 8 unimplementable as written — the CaddyClient trait is missing the method Phase 8 requires, and the storage trait references a table that does not exist in the authoritative schema. These two findings plus F050 (silent task death) represent a design that cannot be built, run reliably, or debugged when it fails.

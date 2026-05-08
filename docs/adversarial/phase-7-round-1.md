# Adversarial Review — Phase 7 — Round 1

**Design summary:** Phase 7 delivers the `CaddyApplier` adapter and `CaddyJsonRenderer` core type, which together take a `Snapshot` carrying a `DesiredState`, render it to Caddy 2.x JSON, post it to Caddy's admin API via `POST /load`, verify the round-trip equivalence, and record exactly one terminal audit row per `correlation_id`. It also introduces a dual-layer locking scheme (in-process `Mutex` + SQLite `apply_locks` table) and a detached `TlsIssuanceObserver` for ACME polling.

**Prior rounds:** None — this is Round 1.

---

## Summary

The most dangerous structural weaknesses in Phase 7 are: (1) the stale-lock reclaim path for the SQLite advisory lock has no covering transaction, allowing two crashed-and-recovered processes to both believe they hold the lock; (2) the sentinel placement specified in the renderer conflicts with the existing codebase's sentinel location, making the post-load equivalence check produce permanent false failures; and (3) the SQLite IMMEDIATE transaction spans an unbounded Caddy HTTP call, violating both the write-lock constraint and the latency SLA simultaneously.

---

## Findings

### F001 — SQLite advisory lock TTL check races the apply body
**Severity:** CRITICAL
**Category:** composition-failure
**Slice:** 7.6

**Attack:** The design describes a stale-lock cleanup strategy that checks `holder_pid` against `/proc/<pid>/status` OR stamps a 5-minute TTL. No mechanism is specified for *who* does this check, *when* it runs, or *what atomically combines* the check with the re-acquisition. Concretely: Process A holds the `apply_locks` row with `holder_pid = 1234`. A is killed mid-apply. Process B starts, calls `INSERT INTO apply_locks` — gets a uniqueness violation. B then executes the stale-lock check: it reads the row, confirms PID 1234 is dead, and attempts a `DELETE + INSERT`. Between B's DELETE and B's INSERT, Process C also detects the stale lock and races to do the same DELETE + INSERT. Both B and C can complete the `INSERT` sequence if there is no covering SQLite transaction around the entire check-delete-insert triple. The result is two concurrent appliers both believing they hold the lock.

**Why the design doesn't handle it:** The design lists the TTL/pid-check as a comment ("OR stamp a 5-minute TTL") with no transaction boundary around the check-then-reclaim cycle. There is no `BEGIN IMMEDIATE` wrapping the stale-lock reclaim path described in Slice 7.6.

**Blast radius:** Two parallel `POST /load` calls reach Caddy in rapid succession. The second may overwrite the first with a different snapshot. The `config_version` pointer still advances once (the Caddy state and the DB pointer diverge). The ownership sentinel equivalence check in 7.4 step 5 does not catch this because both configs are Trilithon-owned.

**Recommended mitigation:** Wrap the entire stale-lock reclaim path in a `BEGIN IMMEDIATE` transaction: `BEGIN IMMEDIATE; SELECT FROM apply_locks WHERE instance_id = ?; -- check pid dead; DELETE FROM apply_locks WHERE instance_id = ? AND holder_pid = ?; INSERT INTO apply_locks ...; COMMIT`. The `rowcount == 1` on the DELETE acts as the compare-and-swap. Alternatively, replace the SQLite advisory lock with an `fs2`-based file lock (acquired in `lock.rs`) that the OS automatically releases on crash, extending it to cover the full apply window.

---

### F002 — Sentinel placement in renderer conflicts with existing codebase; post-load equivalence check permanently fails
**Severity:** HIGH
**Category:** assumption-violation
**Slice:** 7.1 / 7.4 cross-cutting

**Attack:** Slice 7.1 specifies `CaddyJsonRenderer` inserts `"@id": "trilithon-owner-<instance>"` at the **top level** of the rendered Caddy JSON document. However, Caddy's `@id` field is a module-id marker that Caddy normalises internally — it does not survive a `GET /config/` round-trip in the same position. After `POST /load`, when step 5 of Slice 7.4 calls `get_running_config()` and runs the structural diff, the `@id` at the top level will either be absent or repositioned, causing the equivalence check to return `ApplyError::CaddyRejected { "post-load equivalence failed" }` on every successful apply.

**Why the design doesn't handle it:** The design specifies the sentinel at "top level" without reconciling this with known Caddy `@id` normalisation behaviour or with the existing `ensure_sentinel` code path that places the sentinel at `/apps/http/servers/__trilithon_sentinel__` specifically to survive round-trips.

**Blast radius:** Every apply attempt returns a false `config.apply-failed` once any config is present. The system stops converging entirely. The sentinel is written then immediately reported as missing.

**Recommended mitigation:** Define the sentinel placement to match the established `__trilithon_sentinel__` server entry path, consistent with `sentinel.rs`. Add this path to the equivalence diff ignore-list so Caddy normalisation never triggers a false rejection. Remove the top-level `@id` specification from Slice 7.1.

---

### F003 — TLS observer and apply success path both emit `config.applied`, violating the single-terminal-row invariant
**Severity:** HIGH
**Category:** composition-failure
**Slice:** 7.8 / 7.7 cross-cutting

**Attack:** Slice 7.4 step 6 writes one `config.applied` audit row on success. Slice 7.8 step 3 says the observer writes "one `config.applied` audit row with `notes.applied_state = 'tls-issuing' → 'applied'"`. If apply succeeds and new TLS hostnames are added, both paths write `config.applied` under the same `correlation_id`. The Slice 7.7 property test assertion "exactly one of `{config.applied, config.apply-failed, mutation.conflicted}` per `correlation_id`" is structurally impossible to satisfy for TLS paths under the current design.

**Why the design doesn't handle it:** The design does not specify that the 7.4 success row uses a different `applied_state` tag (`tls-issuing`) when TLS observation is pending, nor does it introduce a distinct audit kind for the observer's completion event.

**Blast radius:** The single-terminal-row property test fails for all TLS cases. Operators querying by `correlation_id` see two `config.applied` rows. Downstream logic (alerts, Phase 8 drift detection) that assumes one terminal row per apply is confused.

**Recommended mitigation:** When TLS issuance is triggered, the 7.4 success path writes `config.applied` with `applied_state = "tls-issuing"` as its terminal row. The observer writes a distinct informational event under a new audit kind (`tls.issuance-completed` or `tls.issuance-timeout`) rather than a second `config.applied`. Update the closed audit vocabulary table and the property test invariant accordingly.

---

### F004 — SQLite IMMEDIATE transaction spans unbounded Caddy HTTP call; write lock held for entire `POST /load` duration
**Severity:** HIGH
**Category:** cascade-construction
**Slice:** 7.5

**Attack:** The design specifies: "Open IMMEDIATE-mode transaction before `POST /load`; commit after Caddy returns 200." An `IMMEDIATE` transaction holds the SQLite write lock for its entire duration. If Caddy is slow (e.g., processing a large TLS certificate bundle), the write lock can be held for several seconds. With `busy_timeout = 5000ms`, any other SQLite write arriving after 5 seconds fails with `SQLITE_BUSY`. This includes snapshot inserts, audit row appends, and probe persistence for every concurrent request.

**Why the design doesn't handle it:** The design explicitly requires the transaction to span the Caddy HTTP call to prevent TOCTOU on `config_version`, but acknowledges no timeout on the Caddy call and does not address the interaction with `busy_timeout`.

**Blast radius:** A 5+ second Caddy reload blocks all SQLite writes, cascading into missed audit rows for concurrent operations. This also violates the p95 < 2s latency SLA on every other operation in the system while the slow apply is in flight.

**Recommended mitigation:** Decouple the version read from the Caddy call. Structure: (1) read `MAX(config_version)` in a short read transaction; (2) call Caddy with an explicit per-call timeout (e.g., `tokio::time::timeout(Duration::from_secs(10), client.load_config(body))`); (3) if Caddy succeeds, open `BEGIN IMMEDIATE` only to check version + insert snapshot — a sub-millisecond operation. This is a two-phase optimistic check; the IMMEDIATE section never spans a network call.

---

### F005 — No per-call timeout on `client.load_config`; in-process mutex held indefinitely on hung Caddy
**Severity:** HIGH
**Category:** cascade-construction
**Slice:** 7.4 / 7.6

**Attack:** `client.load_config(body).await` has no timeout specified anywhere in the design. The in-process `instance_mutex` is held for the full duration of the apply body including this call. If Caddy's endpoint is reachable (eliminating `Unreachable`) but slow to respond, the mutex is locked for the full hang duration — potentially unbounded. Every concurrent apply request queues behind it indefinitely. On SIGTERM, the process may hang.

**Why the design doesn't handle it:** No timeout is specified for any Caddy HTTP call in Slice 7.4 or the `CaddyClient` trait specification.

**Blast radius:** All inbound apply requests block behind the hung mutex. The SQLite `apply_locks` row is also stuck. The system appears healthy at the TCP layer but is operationally frozen. Violates the p95 < 2s latency SLA for all concurrent applies.

**Recommended mitigation:** Specify an explicit per-call timeout in the design: `tokio::time::timeout(config.caddy.admin_timeout_secs, client.load_config(body))`. On timeout, surface `ApplyError::Unreachable` (not `CaddyRejected`) and write a `caddy.unreachable` audit row. The timeout value defaults to 10 seconds and is configurable.

---

### F006 — "Trilithon-owned keys" for `unknown_extensions` collision check is undefined; sentinel path silently overwritable
**Severity:** HIGH
**Category:** assumption-violation
**Slice:** 7.1

**Attack:** The design states: "The merge MUST NOT overwrite a Trilithon-owned key. A collision is a programmer error and returns `RenderError`." But the design never defines what "Trilithon-owned keys" are — no set, no path prefix, no enumeration. A caller who passes `unknown_extensions` with key `/apps/http/servers/__trilithon_sentinel__` silently overwrites the sentinel server block. The render succeeds, `POST /load` delivers a config without the sentinel, and the post-load equivalence check does not catch this because both the rendered and live configs lack the sentinel.

**Why the design doesn't handle it:** The owned-key set is described as "Trilithon-owned" but left entirely undefined, making the collision-prevention logic impossible to implement correctly.

**Blast radius:** Ownership sentinel silently dropped from a successful apply. Phase 8 drift detection detects this on next poll, but the apply audit row records success. Violates ADR-0015's sentinel preservation invariant.

**Recommended mitigation:** Define the owned-key set exhaustively as a constant in `CaddyJsonRenderer`: at minimum `["apps/http/servers/__trilithon_sentinel__", "apps/http", "apps/tls"]`. Return `RenderError::OwnedKeyConflict { key }` when any `JsonPointer` in `unknown_extensions` shares a prefix with an owned key.

---

### F007 — `advance_config_version_if_eq` is a separate transaction from `insert_snapshot_inner`; config pointer and DB state can diverge when Caddy succeeds but the version-advance transaction fails
**Severity:** HIGH
**Category:** composition-failure
**Slice:** 7.5

**Attack:** The existing `insert_snapshot_inner` path has its own `BEGIN IMMEDIATE` transaction that checks `MAX(config_version)` and inserts the snapshot row. Slice 7.5 introduces `advance_config_version_if_eq` as a separate function with its own `BEGIN IMMEDIATE`. If these are two distinct transactions, the following sequence is possible: (1) `insert_snapshot_inner` transaction commits (snapshot row written, version incremented by its own CAS); (2) Caddy call succeeds; (3) `advance_config_version_if_eq` opens a second `IMMEDIATE` transaction — now `SQLITE_BUSY` if any other write is in flight — and fails. Result: Caddy is running the new config, the snapshot row exists, but the "current pointer" stored in `advance_config_version_if_eq`'s table differs from the snapshot table's `MAX(config_version)`. The pointer and reality diverge permanently.

**Why the design doesn't handle it:** The design introduces `advance_config_version_if_eq` without specifying whether it is the same transaction as the snapshot insert or a separate one. The two-transaction reading of the design creates this divergence window.

**Blast radius:** Config-version pointer permanently behind actual Caddy state. Every subsequent apply sees a stale `expected_version` and returns `OptimisticConflict` until an operator manually reconciles. Applies stop working.

**Recommended mitigation:** Specify explicitly that the snapshot insert and the config-version pointer advance are a single `BEGIN IMMEDIATE` transaction. `advance_config_version_if_eq` must be called *inside* the existing `insert_snapshot_inner` transaction, not as a separate call.

---

### F008 — `TlsIssuanceObserver` has no cancellation when a superseding apply removes the watched hostname
**Severity:** MEDIUM
**Category:** cascade-construction
**Slice:** 7.8

**Attack:** Apply A adds `foo.example.com` and spawns a `TlsIssuanceObserver` for `(correlation_id_A, ["foo.example.com"])`. Apply B then removes `foo.example.com` from desired state. Observer A still polls `get_certificates()` for up to 120 seconds. If ACME eventually issues a cert for `foo.example.com` (e.g., Caddy had already started the ACME challenge before Apply B was applied), Observer A writes `config.applied` (or `config.apply-failed`) for `correlation_id_A` referencing a hostname that is no longer in desired state. This produces a misleading terminal audit row.

**Why the design doesn't handle it:** The observer has no cancellation mechanism and no check against current desired state before writing its completion row.

**Blast radius:** Misleading audit rows. Operators see `config.applied` for removed hostnames. Phase 8 drift detection may misidentify the state.

**Recommended mitigation:** Bind each observer to a `CancellationToken` stored in a map keyed by `(instance_id, hostname)`. When an apply removes a hostname, the applier cancels the token before writing the success row. Alternatively, the observer verifies — before writing its completion row — that the hostname still exists in the current snapshot.

---

### F009 — `latency_ms: u32` in `ApplyOutcome::Succeeded` silently truncates on overflow
**Severity:** MEDIUM
**Category:** assumption-violation
**Slice:** 7.2

**Attack:** `latency_ms: u32` max value is ~49.7 days. Under clock skew, test environments, or a bug in the `Clock` implementation, the millisecond difference between `start` and `end` may exceed `u32::MAX`. A naïve `(end - start) as u32` silently truncates to a small value. The audit row and tracing span record the corrupted value. The p95 < 2s latency SLA cannot be monitored from corrupted data.

**Why the design doesn't handle it:** The design specifies `latency_ms: u32` without documenting the conversion from the `clock.now()` difference or what happens on overflow.

**Blast radius:** Corrupt latency values in audit rows and tracing spans. No functional failure, but operator visibility for SLA monitoring is degraded.

**Recommended mitigation:** Change `latency_ms` to `u64`, or use `u32::try_from(ms).unwrap_or(u32::MAX)` with a `tracing::warn!` on clamp. Document the computation as `clock.elapsed_ms_since(start).min(u64::from(u32::MAX)) as u32`.

---

### F010 — Capability re-check is characterised as a hard gate but is actually a best-effort heuristic; wrong audit kind emitted when the miss is caught by Caddy
**Severity:** MEDIUM
**Category:** assumption-violation
**Slice:** 7.3 / 7.4

**Attack:** The capability re-check reads a clone from `CapabilityCache` (a `RwLock`). Between the clone and `POST /load`, the background probe can refresh the cache with a missing module. The `POST /load` reaches Caddy with a config referencing an absent module; Caddy returns 4xx. This is handled by the `CaddyValidation` failure path — but the applier then emits `ApplyFailureKind::CaddyValidation`, not `ApplyFailureKind::CapabilityMismatch`. Operators and monitoring see the wrong failure kind, and the audit row records `error_kind = "CaddyValidation"` when the root cause was a capability miss.

**Why the design doesn't handle it:** The design treats the capability re-check as a definitive gate rather than a short-circuit optimisation. No mechanism is specified to parse the Caddy 4xx body and upgrade the failure kind to `CapabilityMismatch` when appropriate.

**Blast radius:** Incorrect audit kinds for capability-miss failures. Operators cannot distinguish validation errors from capability mismatches in the audit log. Any alert or dashboard that counts `CapabilityMismatch` events under-counts real module misses.

**Recommended mitigation:** Document the capability re-check as a best-effort short-circuit, not a hard gate. Add a Caddy 4xx body parser that looks for module-not-found patterns and maps them to `ApplyFailureKind::CapabilityMismatch { missing_module }`. This ensures the correct audit kind regardless of which path catches the miss.

---

## Severity summary

| Severity | Count |
|----------|-------|
| CRITICAL | 1 |
| HIGH     | 6 |
| MEDIUM   | 3 |
| LOW      | 0 |

**Top concern:** F001 — the stale-lock reclaim path in the SQLite advisory lock has no covering transaction, creating a concrete race where two processes both believe they hold the lock after a crash, allowing parallel `POST /load` calls and a diverged `config_version` pointer.

**Must address before implementation:** F001 (lock reclaim race), F002 (sentinel placement/equivalence false positive), F004 (IMMEDIATE transaction spanning HTTP call), F007 (snapshot insert and version-advance in separate transactions).

# Adversarial Review — Phase 7 — Round 3

**Prior rounds:** Round 1 (F001–F010), Round 2 (F011–F019). All unaddressed.

---

## Summary

Round 3 examined the `rollback()`/`validate()` stubs, the dual conflict representation, the undefined `ValidationReport` type, secret leakage through `error_detail`, detached observer task lifecycle, float serialisation portability, and the stringly-typed `ApplyError::Storage`. The most dangerous new finding (F020) shows that `rollback()` shares no lock acquisition with `apply()`, enabling Phase 12 to race an in-flight apply and produce the same compound damage as F017 through a path F017 did not cover.

---

## Findings

### F020 — `rollback()` is not required to acquire the per-instance mutex and advisory lock; races a concurrent `apply()`
**Severity:** CRITICAL
**Category:** composition-failure
**Slice:** 7.4 / 7.6

**Attack:** The `Applier` trait declares `rollback()` as a peer method to `apply()`. The in-process `Mutex` and SQLite advisory lock acquisition are documented only inside `apply()`. Phase 12 will implement `rollback()` as a standalone code path that fetches the target snapshot and calls `client.load_config(...)`. If Phase 12 does not acquire the same locks (there is no cross-reference requiring it to), `rollback()` and a concurrent `apply()` both reach `POST /load` and `advance_config_version_if_eq` without mutual exclusion. The compound damage is identical to F017: two `POST /load` calls, two version pointer advances, a broken snapshot chain.

**Why the design doesn't handle it:** The lock acquisition invariant is buried inside `apply()` with no trait-level documentation. Phase 12's entry conditions do not reference slice 7.6's locking invariant. There is no shared `async fn acquire_apply_lock()` on the concrete struct that both methods would call.

**Blast radius:** Two concurrent operations advance `config_version` simultaneously. The snapshot chain becomes permanently ambiguous. Phase 8 drift detection and Phase 12 rollback itself operate on indeterminate state. Same blast radius as F017 but through the rollback path.

**Recommended mitigation:** Add an explicit invariant at the `Applier` trait definition: "Implementations MUST acquire the per-instance mutex and advisory lock before issuing any `POST /load` or advancing the version pointer, regardless of which method (apply, rollback) initiated the operation." Encode this as a shared `async fn acquire_apply_lock(&self) -> LockGuard` on `CaddyApplier`, called at the top of both `apply()` and `rollback()`. Add Phase 12 as a dependency that must reference this invariant.

---

### F021 — `ApplyOutcome::Conflicted` and `ApplyError::OptimisticConflict` are both defined; callers matching only one arm silently return 500 for the other
**Severity:** HIGH
**Category:** assumption-violation
**Slice:** 7.2 / 7.5

**Attack:** The `Applier` trait returns `Result<ApplyOutcome, ApplyError>`. Both `ApplyOutcome::Conflicted { stale_version, current_version }` and `ApplyError::OptimisticConflict { observed_version, expected_version }` exist in the type system. The design sends concurrency conflicts down the `Err` path (slice 7.5, step 3). Phase 9 writes `match result { Err(ApplyError::OptimisticConflict { .. }) => 409, ... }`. If a future caller pattern-matches on `Ok(ApplyOutcome::Conflicted { .. })` expecting 409, the match arm is never reached. Conversely, if the implementation accidentally returns `Ok(Conflicted)`, the `mutation.conflicted` audit row (written on the `Err` path in step 3) is silently skipped.

**Why the design doesn't handle it:** The design never states which of the two conflict representations is canonical. Both variants exist in scope; any implementer will choose one; the other remains reachable and untested.

**Blast radius:** Concurrency conflicts surface as 500 for callers that match the wrong arm. The `mutation.conflicted` audit row may be skipped. Phase 9 and Phase 12 HTTP handlers, written at different times, produce inconsistent HTTP semantics for the same underlying event.

**Recommended mitigation:** Remove `ApplyOutcome::Conflicted` entirely. All concurrency conflicts surface as `Err(ApplyError::OptimisticConflict)`. Update all call sites and the audit row write to use the `Err` path exclusively. Document the rule in `Applier` trait docs.

---

### F022 — `ValidationReport` is undefined in Phase 7; Phase 12 filling it in is a breaking API change on the `Applier` trait
**Severity:** HIGH
**Category:** assumption-violation
**Slice:** 7.4

**Attack:** The `Applier` trait declares `validate() -> Result<trilithon_core::reconciler::ValidationReport, ApplyError>`. Neither `ValidationReport` nor `ApplyError::PreflightFailed` (the placeholder return) are defined in Phase 7's type inventory. When Phase 12 fills these in, every mock and test double written in Phases 8–11 that implements `validate()` must be updated. If Phase 12 defines `ValidationReport` in `core::preflight`, the `Applier` trait in `core::reconciler` imports from a peer module — an undocumented forward dependency.

**Why the design doesn't handle it:** The design defers the type's definition without specifying where it will live, what it contains, or which module will own it.

**Blast radius:** All `CaddyApplier` test doubles in Phases 8–11 break when Phase 12 ships. The `ApplyError::PreflightFailed` variant referenced in the stub does not compile — it is not in the Phase 7 `ApplyError` enum definition.

**Recommended mitigation:** Define `ValidationReport` as an empty opaque struct in `core::reconciler::applier` in Phase 7. Add `ApplyError::PreflightFailed { failures: Vec<String> }` to the `ApplyError` enum. Document in Phase 7's open questions that Phase 12 must fill in the type without changing the method signature. The stub compiles and implements compile correctly; Phase 12 extends the type.

---

### F023 — `error_detail: Option<String>` in `ApplyAuditNotes` is populated from Caddy's 4xx body, which may echo the submitted config including upstream credentials
**Severity:** HIGH
**Category:** data-exposure
**Slice:** 7.7

**Attack:** `ApplyAuditNotes.error_detail` is populated from `ApplyError::CaddyRejected { detail }`. The `detail` is the bounded excerpt from Caddy's 400 response body. Caddy's validation error responses routinely echo the submitted JSON payload. If a route's upstream URL is `http://admin:secretpassword@internal-db:5432`, that URL appears in Caddy's error body, flows into `detail`, and is persisted in `audit_log.notes` as plain text. The `RedactedDiff` newtype guards the diff column but does not guard the `notes` column.

**Why the design doesn't handle it:** `error_detail` is typed as `Option<String>` with no redaction step. The design's `AuditWriter` constraint ("accepts only `RedactedDiff`") applies to the diff payload, not to the `notes` JSON blob.

**Blast radius:** The audit log is append-only; leaked credentials cannot be deleted. Any audit export, read replica, or future compliance report receives the leaked material. The `secrets.revealed` event does not fire — this is an unintentional, untracked leak.

**Recommended mitigation:** Introduce `BoundedErrorDetail(String)` that truncates to 512 bytes and strips basic-auth credentials from URLs (using the same redactor pattern used for diffs). Type `ApplyAuditNotes.error_detail` as `Option<BoundedErrorDetail>`. Populate it by passing `detail` through a `redact_error_detail()` function before constructing `ApplyAuditNotes`.

---

### F024 — Detached `TlsIssuanceObserver` tasks are not registered with the shutdown `JoinSet`; active observers at SIGTERM leave uncommitted SQLite writes
**Severity:** HIGH
**Category:** cascade-construction
**Slice:** 7.8

**Attack:** `TlsIssuanceObserver::observe` is spawned as a detached `tokio::spawn` task. The `run.rs` drain path (`drain_tasks`) only joins tasks in its `JoinSet`. A SIGTERM arriving while an observer is mid-poll (within the 120-second window) is not propagated to the observer. The `drain_tasks` budget (10 s) expires; `JoinSet::abort_all()` is called on the tracked tasks; the daemon exits. The observer task holds `Arc<AuditWriter>`, which holds `Arc<SqlitePool>`. The pool is still live because the observer holds a reference. The observer's in-flight `audit.record(...)` call — a `BEGIN IMMEDIATE` write — is dropped mid-execution. On next startup, SQLite WAL recovery completes cleanly, but the terminal audit row for that TLS issuance (completion or timeout) is never written, violating the single-terminal-row invariant.

**Why the design doesn't handle it:** The design says the observer "never blocks the original `apply()` return" but does not specify how the observer participates in graceful shutdown. No `ShutdownSignal` parameter, no `JoinHandle` registration.

**Blast radius:** For any TLS issuance in progress at shutdown time: the "exactly one terminal audit row per `correlation_id`" invariant is violated. The `Arc<SqlitePool>` prevents clean pool close during shutdown. Multiple outstanding observers accumulate unbounded `Arc` references to shared resources.

**Recommended mitigation:** Pass a `CancellationToken` into `TlsIssuanceObserver::observe`. The poll loop selects on `tokio::select!` between the 5-second interval and the cancellation signal. On cancellation, emit a `config.apply-failed` row with `error_kind = "ObserverShuttingDown"` before returning. Register all observer `JoinHandle`s with the supervisor's `JoinSet` so `drain_tasks` can abort them within `DRAIN_BUDGET`.

---

### F025 — `canonical_json_bytes` delegates `f64` serialisation to `serde_json`, which changes algorithm across minor versions; non-integer floats in `unknown_extensions` produce non-portable snapshot addresses
**Severity:** MEDIUM
**Category:** assumption-violation
**Slice:** 7.1

**Attack:** `canonical_json_bytes` normalises whole-valued floats to integers but passes non-integer `Value::Number` floats directly to `serde_json::to_vec`. `serde_json >= 1.0.9` uses Ryu for `f64`-to-string conversion; older versions use a different algorithm. If a workspace dependency resolves `serde_json` to a version below 1.0.9 (e.g. via Cargo minimum-version resolution), the same `f64` value in `unknown_extensions` produces a different byte string. The `SnapshotId` content address changes silently. Snapshots written before and after the version boundary have different addresses for logically identical states. Phase 8 drift detection generates false positives for all such snapshots.

**Why the design doesn't handle it:** The design guarantees "byte-identical inputs produce byte-identical outputs" without specifying which serialisation algorithm or minimum `serde_json` version enforces it. The `CANONICAL_JSON_VERSION` constant requires a manual bump on format changes; no mechanism enforces format stability across `serde_json` version boundaries.

**Blast radius:** Silent content-address corruption. No error is produced. Phase 8 drift detection and Phase 12 rollback reachability both depend on content addresses being stable. Forensic audit queries using snapshot IDs return no results for states whose addresses changed.

**Recommended mitigation:** Add `serde_json = ">=1.0.9"` as a lower-bound constraint in `Cargo.toml` and document the Ryu dependency in `canonical_json.rs`. Add a property test asserting that `canonical_json_bytes` output for a `Value` containing non-integer floats is byte-identical across 1000 random inputs (cross-check against a reference output baked into the test).

---

### F026 — `ApplyError::Storage(String)` erases the `StorageError` discriminant; a CAS failure from `advance_config_version_if_eq` surfaces as 500 instead of 409
**Severity:** MEDIUM
**Category:** assumption-violation
**Slice:** 7.2

**Attack:** `ApplyError::Storage(String)` is a stringly-typed catch-all. If the applier maps `StorageError` to `ApplyError::Storage` via `.map_err(|e| ApplyError::Storage(e.to_string()))` rather than pattern-matching on `StorageError::OptimisticConflict`, a CAS failure from `advance_config_version_if_eq` surfaces as `ApplyError::Storage("optimistic conflict: ...")` instead of `ApplyError::OptimisticConflict`. Phase 9's HTTP handler matches on `Err(ApplyError::OptimisticConflict)` for 409; the catch-all `Err(_)` returns 500. The `mutation.conflicted` audit row is written before the mapping occurs, so the audit log is correct — but the HTTP response is wrong.

**Why the design doesn't handle it:** The design defines both `ApplyError::OptimisticConflict` and `ApplyError::Storage(String)` without specifying which `StorageError` variants must be explicitly pattern-matched into typed `ApplyError` variants before falling through to the string-erased form.

**Blast radius:** Concurrency conflicts return 500 to the caller. Phase 9 and Phase 12 conflict-resolution UI never fires. Test doubles that mock the storage layer may return the correct typed error, masking the bug until production.

**Recommended mitigation:** Remove `ApplyError::Storage(String)`. Replace with `ApplyError::StorageInternal` (no payload, for opaque failures). Add a typed mapping function `fn map_storage_error(e: StorageError) -> ApplyError` that explicitly pattern-matches `StorageError::OptimisticConflict` → `ApplyError::OptimisticConflict` and all other variants → `ApplyError::StorageInternal`. Test this function independently of the applier.

---

## Severity summary

| Severity | Count |
|----------|-------|
| CRITICAL | 1 (F020) |
| HIGH     | 4 (F021, F022, F023, F024) |
| MEDIUM   | 2 (F025, F026) |
| LOW      | 0 |

**Top concern:** F020 — `rollback()` shares no lock acquisition invariant with `apply()`. When Phase 12 implements `rollback()`, it will race concurrent applies and produce the same broken-snapshot-chain outcome as F017 through an uncovered path.

**Must address before Phase 12 begins:** F020. **Must address before Phase 9 writes HTTP handlers:** F021. **Must address before Phase 8–11 test doubles are written:** F022.

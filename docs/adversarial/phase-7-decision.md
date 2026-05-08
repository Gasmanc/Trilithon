# Adversarial Design Review — Phase 7 Apply Path — Decision Document

**Design:** Phase 7 — `CaddyApplier` + `CaddyJsonRenderer`
**Rounds completed:** 6 (R1–R6)
**Total findings:** 35 (F001–F035)
**Surface declared exhausted:** Round 6
**Document status:** Final

---

## Round Summary

| Round | Findings | Critical | High | Medium | Low | Surfaces probed |
|-------|----------|----------|------|--------|-----|-----------------|
| R1    | F001–F010 (10) | 1 | 6 | 3 | 0 | Lock reclaim race, sentinel placement, TLS observer/apply audit collision, IMMEDIATE-spans-HTTP, per-call timeouts, owned-key set, snapshot+version-advance separation, observer cancellation, latency overflow, capability-check heuristic |
| R2    | F011–F019 (9)  | 1 | 4 | 4 | 0 | AuditWriter failure on success leg, step-5 failure after step-4 success, `mutation.conflicted` wrong kind, DiffEngine ignore-list undefined, reclaim livelock, TOCTOU on two-phase version check, F001+F004+F007 compound, `unknown_extensions` size bound, TLS observer baseline race |
| R3    | F020–F026 (7)  | 1 | 4 | 2 | 0 | `rollback()` lock invariant, dual conflict representation, `ValidationReport` undefined, credential leakage in `error_detail`, detached observer shutdown gap, `f64` serialisation portability, `ApplyError::Storage(String)` discriminant erasure |
| R4    | F027–F031 (5)  | 0 | 1 | 3 | 1 | Hostname leakage in `Debug`, schema precondition on `apply_locks`, `instance_id` no validation, `ReloadKind::Abrupt` dead variant, `snapshot_id` hashes wrong representation |
| R5    | F032–F034 (3)  | 0 | 2 | 1 | 0 | TLS observer timing in property test, `AuditWriter` backpressure unspecified, `rollback()` cross-instance `SnapshotId` |
| R6    | F035 (1)       | 0 | 0 | 0 | 1 | `AppliedStateTag` serialised strings not pinned |
| **Total** | **35** | **3** | **17** | **12** | **3** | |

---

## Complete Findings Inventory

### Critical

| ID   | Category             | Slices   | Title |
|------|----------------------|----------|-------|
| F001 | composition-failure  | 7.6      | SQLite advisory lock TTL check races the apply body — two processes can both believe they hold the lock after a crash |
| F017 | cascade-construction | 7.5/7.6/7.7 | F001 + F004 + F007 compound — two processes simultaneously advance `config_version` to the same value, leaving the snapshot chain permanently ambiguous |
| F020 | composition-failure  | 7.4/7.6  | `rollback()` not required to acquire per-instance mutex and advisory lock — races a concurrent `apply()`, same blast radius as F017 |

### High

| ID   | Category             | Slices   | Title |
|------|----------------------|----------|-------|
| F002 | assumption-violation | 7.1/7.4  | Sentinel at top-level conflicts with Caddy normalisation; post-load equivalence check permanently fails |
| F003 | composition-failure  | 7.7/7.8  | TLS observer and apply success path both emit `config.applied`; single-terminal-row invariant violated for all TLS paths |
| F004 | cascade-construction | 7.5      | SQLite IMMEDIATE transaction spans unbounded Caddy HTTP call; write lock held for entire `POST /load` duration |
| F005 | cascade-construction | 7.4/7.6  | No per-call timeout on `client.load_config`; in-process mutex held indefinitely on hung Caddy |
| F006 | assumption-violation | 7.1      | "Trilithon-owned keys" for `unknown_extensions` collision check is undefined; sentinel path silently overwritable |
| F007 | composition-failure  | 7.5      | `advance_config_version_if_eq` is a separate transaction from `insert_snapshot_inner`; pointer and DB state can diverge |
| F011 | assumption-violation | 7.4/7.7  | `AuditWriter` failure on success leg unspecified; apply returns `Succeeded` but no terminal row written |
| F012 | assumption-violation | 7.4      | `get_running_config` failure after successful `POST /load` surfaces `ApplyError::Unreachable`; Caddy serves new config but pointer not advanced |
| F014 | abuse-case           | 7.4      | `DiffEngine` ignore-list undefined; over-broad list makes post-load equivalence check a security bypass |
| F015 | composition-failure  | 7.6      | F001 reclaim mitigation (`BEGIN IMMEDIATE` for DELETE+INSERT) creates livelock when reclaimer and new acquirer race |
| F016 | composition-failure  | 7.5      | F004 two-phase mitigation has TOCTOU: version check outside `BEGIN IMMEDIATE` does not protect against concurrent version advance |
| F021 | assumption-violation | 7.2/7.5  | `ApplyOutcome::Conflicted` and `ApplyError::OptimisticConflict` both defined; callers matching only one arm silently return 500 for the other |
| F022 | assumption-violation | 7.4      | `ValidationReport` undefined in Phase 7; Phase 12 filling it in is a breaking API change on the `Applier` trait |
| F023 | data-exposure        | 7.7      | `error_detail: Option<String>` populated from Caddy 4xx body, which may echo submitted config including upstream credentials |
| F024 | cascade-construction | 7.8      | Detached `TlsIssuanceObserver` tasks not registered with shutdown `JoinSet`; active observers at SIGTERM leave uncommitted writes and violate the terminal-row invariant |
| F028 | assumption-violation | 7.5/7.6  | `apply_locks` table may not exist when `CaddyApplier::apply` first runs; lock silently not acquired and apply proceeds unguarded |
| F033 | assumption-violation | 7.4/7.7  | `AuditWriter` bounded-channel backpressure semantics unspecified; blocking or dropping both produce concrete SLA or audit failures |
| F034 | abuse-case           | 7.4      | `rollback()` accepts any `SnapshotId` without `caddy_instance_id` scoping; cross-instance snapshot applied silently — exploitable via LLM gateway |

### Medium

| ID   | Category             | Slices   | Title |
|------|----------------------|----------|-------|
| F008 | cascade-construction | 7.8      | `TlsIssuanceObserver` has no cancellation when superseding apply removes watched hostname |
| F009 | assumption-violation | 7.2      | `latency_ms: u32` silently truncates on overflow |
| F010 | assumption-violation | 7.3/7.4  | Capability re-check is best-effort heuristic but emits wrong audit kind when miss caught by Caddy |
| F013 | assumption-violation | 7.5      | `mutation.conflicted` audit kind semantically wrong for apply-time concurrency abort |
| F018 | abuse-case           | 7.1      | `unknown_extensions` has no size bound; crafted entries exhaust heap during render and leave advisory lock held on OOM kill |
| F019 | assumption-violation | 7.8      | `TlsIssuanceObserver` reads previous snapshot from live storage at spawn time; concurrent apply invalidates baseline |
| F025 | assumption-violation | 7.1      | `canonical_json_bytes` delegates `f64` serialisation to `serde_json` which changes algorithm across minor versions; non-integer floats produce non-portable snapshot addresses |
| F026 | assumption-violation | 7.2      | `ApplyError::Storage(String)` erases `StorageError` discriminant; CAS failure surfaces as 500 instead of 409 |
| F027 | data-exposure        | 7.2/7.8  | `AppliedState::TlsIssuing { hostnames: Vec<String> }` leaks managed domain names into tracing logs via derived `Debug` |
| F029 | assumption-violation | 7.4/7.6  | `instance_id: String` has no validation; empty or malformed values corrupt lock key and sentinel |
| F031 | logic-flaw           | 7.1/7.5  | `snapshot_id` hashes Trilithon canonical JSON but bytes sent to Caddy are structurally different; operators cannot verify running config against a `snapshot_id` |
| F032 | composition-failure  | 7.7/7.8  | TLS observer writes terminal row asynchronously; `correlation_id` property-test invariant is timing-dependent |

### Low

| ID   | Category             | Slices   | Title |
|------|----------------------|----------|-------|
| F030 | logic-flaw           | 7.2/7.7  | `ReloadKind::Abrupt` is silently dead; future abrupt reloads misattributed as graceful in audit log |
| F035 | assumption-violation | 7.7      | `AppliedStateTag` serialised strings not pinned; downstream rename breaks all historical audit queries silently |
| F036 _(none)_ | — | — | _(no additional findings; R6 surface exhausted)_ |

---

## Cluster Analysis

### Cluster A — Locking and Atomicity (F001, F004, F007, F015, F016, F017, F020)

**The highest-impact cluster. Requires a coordinated fix; partial fixes leave compound windows open (F017).**

Root cause: the advisory lock reclaim path lacks a covering transaction (F001), the IMMEDIATE transaction spans the Caddy HTTP call (F004), and the snapshot insert and version-advance are separate transactions (F007). F015 shows the F001 mitigation itself has a livelock. F016 shows the F004 mitigation has TOCTOU. F017 is the compound of all three. F020 is the same compound applied to the Phase 12 `rollback()` path.

**Required coordinated fix:**
1. Replace the SQLite advisory lock with an OS-level `fs2` file lock (crash-safe by construction; no TTL check needed).
2. Make the snapshot insert and version-advance pointer a single `BEGIN IMMEDIATE` transaction — never split.
3. Issue `POST /load` only after that transaction commits (reverse the order: commit DB first, then Caddy).
4. Add a shared `async fn acquire_apply_lock(&self) -> LockGuard` on `CaddyApplier`; document at the `Applier` trait level that both `apply()` and `rollback()` MUST call it before any `POST /load` or version-advance.

### Cluster B — Audit Consistency (F003, F011, F012, F013, F033)

**Affects the single-terminal-row invariant that the property test verifies.**

- F003: TLS success path writes `config.applied` with `applied_state = "tls-issuing"`; observer writes a new kind (`tls.issuance-completed` / `tls.issuance-timeout`), not a second `config.applied`.
- F011: Audit write failure on success leg → return `Succeeded`, emit `tracing::error!`, increment `trilithon_audit_write_failures_total`. Lossy-audit tradeoff is acceptable and must be documented.
- F012: Step-5 failure after step-4 success is a distinct branch: record `config.applied` (load succeeded), warn about equivalence-check failure, do not advance version pointer.
- F013: Add `"config.apply-conflicted"` to `AUDIT_KINDS` vocabulary; use it for optimistic-concurrency aborts at apply time.
- F033: Specify `AuditWriter::record` uses `try_send`, returns `AuditWriteError::ChannelFull` on backpressure. Caller treats this as `ApplyError::Storage` → `ApplyOutcome::Failed`. Channel capacity: 256 entries. Document in spec.

### Cluster C — Security and Data Exposure (F006, F014, F023, F034)

**Two findings are actively exploitable via the LLM tool gateway (F014, F034).**

- F006: Define owned-key set as a compile-time constant in `CaddyJsonRenderer`: at minimum `["apps/http/servers/__trilithon_sentinel__", "apps/http", "apps/tls"]`. Return `RenderError::OwnedKeyConflict { key }` on prefix match.
- F014: Enumerate the DiffEngine ignore-list exhaustively in the Slice 7.4 spec. Any path whose exclusion permits undetected semantic changes to routes, upstreams, or TLS config MUST NOT appear on the list. Compile-time constant.
- F023: Introduce `BoundedErrorDetail(String)` — truncated to 512 bytes, basic-auth credentials stripped. Type `ApplyAuditNotes.error_detail` as `Option<BoundedErrorDetail>`.
- F034: Rollback snapshot query MUST filter by `caddy_instance_id`: `SELECT ... WHERE snapshot_id = ? AND caddy_instance_id = ?`. Return typed `ApplyError::SnapshotNotInLineage` if snapshot exists but belongs to different instance.

### Cluster D — Type System Integrity (F021, F022, F026)

**Must be resolved before Phase 9 writes HTTP handlers and before Phase 8–11 test doubles are written.**

- F021: Remove `ApplyOutcome::Conflicted` entirely. All concurrency conflicts surface as `Err(ApplyError::OptimisticConflict)`. Single canonical conflict representation.
- F022: Define `ValidationReport` as an empty opaque struct in `core::reconciler::applier` in Phase 7. Add `ApplyError::PreflightFailed { failures: Vec<String> }`. Phase 12 extends the type without changing the signature. The stub must compile today.
- F026: Remove `ApplyError::Storage(String)`. Replace with `ApplyError::StorageInternal` (no payload). Add `fn map_storage_error(e: StorageError) -> ApplyError` that explicitly matches `StorageError::OptimisticConflict → ApplyError::OptimisticConflict`; all other variants → `ApplyError::StorageInternal`. Test this function independently.

### Cluster E — Observer Lifecycle (F008, F019, F024, F032)

**All four interact with the TLS observer design.**

- F024: Pass `CancellationToken` into `TlsIssuanceObserver::observe`. Poll loop uses `tokio::select!` on tick vs. cancellation. On cancellation, emit `config.apply-failed` with `error_kind = "ObserverShuttingDown"`. Register all observer `JoinHandle`s with supervisor's `JoinSet`.
- F032: Inject mock observer in property test to record writes synchronously. Add separate integration test that awaits observer via a test-visible `JoinSet` before asserting the terminal-row invariant.
- F008: Bind each observer to a `CancellationToken` stored in a map keyed by `(instance_id, hostname)`. When a superseding apply removes a hostname, cancel the token before writing the new success row.
- F019: Pass parent snapshot directly into observer-spawn logic (loaded once at `apply()` entry from `parent_id` on the new snapshot). Do not re-fetch from live storage.

---

## Must-Fix Before Implementation Starts

The following must be incorporated into the slice specifications before any Phase 7 code is written. They are ordered by dependency.

1. **F017 coordinated fix** (requires F001 + F004 + F007 + F015 + F016 resolved together via Cluster A fix) — any partial fix leaves a compound window.
2. **F020** — `rollback()` lock invariant. Document at `Applier` trait level; implement shared `acquire_apply_lock` on `CaddyApplier`. Phase 12 MUST reference this before `rollback()` is implemented.
3. **F034** — Add `caddy_instance_id` predicate to rollback snapshot query before Phase 7 storage layer is implemented.
4. **F033** — Specify `AuditWriter` backpressure contract (channel capacity, `try_send`, `ApplyError::Storage` mapping) before Slice 7.4 is coded.
5. **F021** — Remove `ApplyOutcome::Conflicted` before any caller code is written; a dead arm is worse than a missing arm.
6. **F022** — Define `ValidationReport` stub and `ApplyError::PreflightFailed` before Phase 8–11 test doubles are written.
7. **F028** — Add schema precondition check in `CaddyApplier::new` (assert `apply_locks` exists). Prevents silent unguarded applies in test harnesses.
8. **F002** — Sentinel placement must match `__trilithon_sentinel__` path. Correct the Slice 7.1 spec before the renderer is written.
9. **F006** — Define owned-key set constant before `CaddyJsonRenderer` is written.
10. **F014** — Enumerate DiffEngine ignore-list before the equivalence check is implemented (security property depends on it).

---

## Must-Fix Before Phase 9 Writes HTTP Handlers

- **F013** — Add `"config.apply-conflicted"` to `AUDIT_KINDS` before any HTTP handler matches on audit kinds.
- **F026** — Remove `ApplyError::Storage(String)` before Phase 9 HTTP 409 handling is written.

---

## Must-Fix During Phase 7 Implementation (Not Design Blockers)

These are implementation-time fixes, not design document changes. They must land before Phase 7 tests pass.

| ID   | Fix |
|------|-----|
| F003 | TLS success path writes `applied_state = "tls-issuing"`; observer writes `tls.issuance-completed` |
| F005 | Add `tokio::time::timeout(admin_timeout_secs, ...)` around every Caddy HTTP call |
| F011 | Specify lossy-audit contract in code comments; add `trilithon_audit_write_failures_total` counter |
| F012 | Implement distinct step-5-failure-after-step-4-success branch |
| F023 | Implement `BoundedErrorDetail` redactor |
| F024 | Pass `CancellationToken` into observer; register handles with `JoinSet` |
| F027 | Introduce `SensitiveHostnames(Vec<String>)` newtype with count-only `Debug` |
| F029 | Introduce `InstanceId(String)` newtype with `try_new` constructor rejecting empty/whitespace |
| F031 | Add `caddy_json_hash: Option<String>` to `Snapshot` populated with hash of bytes sent to Caddy |
| F032 | Inject mock observer in property test; add integration test with explicit observer join |
| F035 | Add explicit `#[serde(rename = "...")]` to each `AppliedStateTag` variant; add vocabulary test |

---

## Accepted Findings (Deferred with Rationale)

| ID   | Severity | Acceptance rationale |
|------|----------|----------------------|
| F008 | MEDIUM   | Requires `CancellationToken` infrastructure from F024 (same fix). Address in same commit. Not a design-phase blocker. |
| F009 | MEDIUM   | `u32` overflow requires continuous apply latency > 49 days. Use `saturating_cast` with `tracing::warn!` at implementation time. Not a design blocker. |
| F010 | MEDIUM   | Capability re-check is documented as a best-effort short-circuit. Adding a Caddy-body module-miss parser is an enhancement; misclassified failures are observable in the audit log. Deferred to a Phase 7 polish commit. |
| F018 | MEDIUM   | Size bounds on `unknown_extensions` are implementation constants (`<= 256 entries`, rendered doc `<= 1 MiB`). Add `RenderError::DocumentTooLarge`. Implement at render time, not a design change. |
| F019 | MEDIUM   | Pass parent snapshot at `apply()` entry to observer-spawn. Implementation detail; resolved alongside F024/F008 observer work. |
| F025 | MEDIUM   | Pin `serde_json = ">=1.0.9"` in `Cargo.toml`. Add baked-reference property test. Implementation-time constraint; no design document change needed. |
| F030 | LOW      | `ReloadKind::Abrupt` dead variant. Add code comment "reserved for Phase 12 emergency path." Add a failing test as a reminder for Phase 12. Deferred to Phase 12. |

---

## Key Constraints Surfaced

1. **Lock-before-commit, not lock-around-network**: The IMMEDIATE transaction must never span a network call. Commit DB state first; issue `POST /load` after. This inverts the originally specified order.

2. **Single source of conflict representation**: `ApplyError::OptimisticConflict` is canonical. `ApplyOutcome::Conflicted` must be removed. Every concurrency conflict surfaces as `Err`.

3. **`rollback()` is a first-class apply operation**: It must acquire the same lock guard as `apply()`. This constraint is documented at the trait level, not inside either method.

4. **Audit vocabulary is closed**: `config.apply-conflicted`, `tls.issuance-completed`, and `tls.issuance-timeout` must be added to `AUDIT_KINDS` and `AuditEvent` in the same commit that introduces code emitting them.

5. **Observer is a managed task, not a detached task**: All `TlsIssuanceObserver` handles join with the supervisor's `JoinSet`. No `tokio::spawn` without registration.

6. **Cross-instance rollback requires type-level or query-level enforcement**: `SnapshotId` alone is insufficient as a rollback key. The storage query must include `caddy_instance_id`.

7. **`snapshot_id` identifies Trilithon representation, not Caddy bytes**: A separate `caddy_json_hash` field is needed for forensic verification. Both must be populated at apply time while the schema is being extended.

8. **`AuditWriter` backpressure is a typed error, not a silent drop or a block**: `try_send` + `ApplyError::Storage` on full channel. The channel capacity (256) is part of the design specification.

---

## Sign-off

Six adversarial rounds across 13 failure categories produced 35 findings. The finding rate dropped from 10 (R1) to 1 (R6) — the design surface is exhausted. Three findings are critical; all three require the Cluster A coordinated locking fix. Seventeen findings are high severity; the majority belong to Clusters B–D and have clear, bounded mitigations.

**Phase 7 implementation may begin** after the 10 must-fix items above are incorporated into the slice specifications. F035 and the implementation-time items in the deferred table may be addressed during coding without further design review.

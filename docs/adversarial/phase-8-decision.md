# Design Decision — Phase 8 (Drift Detection Loop)

**Date:** 2026-05-08
**Rounds:** 9
**Total findings:** 87 (F001–F087)
**Final approach:** The drift detector runs as an `Arc<DriftDetector>` shared between the daemon's background tick task and the Phase 9 HTTP layer. Each tick acquires a named `MutexGuard` (held for the full duration of the fetch-diff-record pipeline), fetches the Caddy running config with a mandatory timeout, diffs it against the latest confirmed-applied snapshot, redacts secrets via an injected `Arc<dyn Redactor>` (not from a shared `SchemaRegistry` import in `core`), and writes a drift event only when the post-redaction canonical hash differs from the last confirmed-clean hash. Deduplication resets only on confirmed successful resolution, not on detection; the `drift_events` table is declared in the architecture §6 schema migration and holds both the mapping fields needed by Phase 9 and the `running_state_hash` required for cross-restart deduplication.

---

## Rejected Approaches

| Approach | Rejected because |
|----------|-----------------|
| `run(self)` consuming `DriftDetector` by value | F062: incompatible with Phase 9 `AppState: Arc<DriftDetector>` — `Arc::try_unwrap` fails when the HTTP server holds a clone, making the entire method signature structurally uncompilable with the downstream design |
| Deduplication hash reset on detection rather than confirmed resolution | F001: a single transient mutation-worker failure permanently silences all future drift alerts for the same config state; hash must reset only after confirmed successful storage write |
| Pre-redaction SHA-256 hash for deduplication | F024/F005: secret rotations (e.g., hourly credential rotation) produce a new hash every tick even when structural config is unchanged, generating 60+ rows/hour of identical-looking audit entries; hash must be computed over post-redaction canonical JSON |
| `CADDY_MANAGED_PATH_PATTERNS` regexes applied to flattened `DesiredState` paths | F077: patterns were written for Caddy-JSON path space (`/apps/tls/...`) but `structural_diff` operates in `DesiredState` schema space (`/version`, `/routes/...`); filter matches nothing, and Trilithon metadata fields (`version`, timestamps) appear as permanent false-positive `Modified` entries |
| `/storage/.*` ignore rule covering the full `/storage/` namespace | F026: this pattern matches `/storage/trilithon-owner` (the ADR-0015 ownership sentinel), making sentinel removal completely invisible to the drift detector; the rule must be narrowed to specific Caddy-owned sub-paths (`/storage/acme/`, `/storage/ocsp/`) with an explicit carve-out that raises `caddy.ownership-sentinel-conflict` for sentinel paths |
| `adopt_running_state` cloning Caddy running state without sentinel check | F018: cloning a sentinel-free running state and applying it removes the sentinel permanently; the next daemon restart re-claims or yields to a competing Trilithon instance, nullifying ADR-0015 multi-instance protection with a single operator action |
| `Mutation::ReplaceDesiredState` and `Mutation::DriftDeferred` as new mutation variants without `expected_version` | F016/F067: these variants do not exist in the authoritative 13-variant `Mutation` enum; adding them without `expected_version` defeats the optimistic concurrency guard, allowing stale drift resolutions to silently overwrite legitimate config changes |
| `run(self) -> ()` with no supervision or restart path | F050: a panic anywhere in the tick pipeline drops the detector struct, resets all in-memory state, and exits silently with no audit row and no observable signal; callers cannot detect the failure |
| `DriftEvent.redacted_diff_json: String` with the pre-serialized string double-passed to `AuditWriter` | F078: `AuditWriter` re-runs the redactor on already-redacted data, recording `redaction_sites = 0` in every audit row regardless of actual secret count; Phase 8 must pass the raw unredacted `Diff` to `AuditWriter` and derive `redacted_diff_json` from the writer's output |
| `unknown_extensions: BTreeMap<JsonPointer, serde_json::Value>` round-tripped through `canonical_json()` for diffing | F076: the flattener escapes `/` in `JsonPointer` keys to `~1`, producing double-encoded paths (`/unknown_extensions/~1apps~1http~1...`) that match no ignore-list patterns and cannot be used for any resolution action; `unknown_extensions` must be excluded from the flat-map diff or expanded inline using the pointer as a pre-formed prefix |

---

## Key Constraints Surfaced

The adversarial process revealed these constraints that any implementation MUST respect:

1. `DriftDetector::run` must take `Arc<Self>` (not `self` by value); `tick_once`, `record`, and `mark_resolved` must operate through shared references so the HTTP layer and the background task can coexist via `Arc::clone`.
2. The apply `MutexGuard` must be bound to a named variable that lives for the full duration of `tick_once` — from `try_lock` through the return of `Drifted` or `Clean`; a temporary dropped at the `if` expression boundary provides zero mutual exclusion.
3. `CaddyClient` trait must add `get_running_config() -> Result<CaddyConfig, CaddyError>` with a mandatory timeout shorter than the minimum tick interval; the timeout must cause `tick_once` to release the apply mutex and return a `TickOutcome::FetchTimeout` variant rather than hanging.
4. The deduplication hash must be computed over post-redaction canonical JSON (not pre-redaction), so that secret-field-only changes do not generate audit-log noise and so the stored hash is comparable to the diff visible in the audit row.
5. The deduplication hash (`last_running_hash`) resets only after `record()` has successfully completed both the `AuditWriter` write and the `storage.record_drift_event` write; neither partial failure nor mutation-worker failure may cause the hash to be set.
6. On daemon startup, `last_running_hash` must be initialised from `storage.latest_unresolved_drift_event(instance_id)` to avoid writing duplicate detection rows across routine daemon restarts.
7. `DriftDetectorConfig` must have a `validate()` method that enforces the [10, 3600] seconds interval constraint at construction time; both zero (panic) and extremely large values (silent dead detector) must be rejected at config-load time before `tokio::time::interval` is ever called.
8. The `drift_events` table must be declared in the architecture §6 schema with explicit columns for `running_state_hash`, `diff_summary` (serialized), `redacted_diff_json`, `redaction_sites`, and `resolution`/`resolved_at`; the `Storage` trait must add `record_drift_event`, `latest_unresolved_drift_event`, and `resolve_drift_event` methods covering the full detection-to-resolution lifecycle.
9. `DiffEngine::redact_diff` must be removed from the `DiffEngine` trait and placed in a separate `RedactionEngine` trait accepting an `Arc<dyn Redactor>`; `DefaultDiffEngine` must hold `pub redactor: Arc<dyn Redactor>` and `SchemaRegistry` must not be imported in `core/crates/core` to preserve the three-layer invariant.
10. `ObjectKind` must derive `Ord` and `PartialOrd`; `BTreeMap<ObjectKind, DiffCounts>` is a compile error without them; alternatively the design must specify `HashMap` and accept non-deterministic serialisation order.
11. The `/storage/.*` ignore-list pattern must be replaced with specific `^/storage/acme/` and `^/storage/ocsp/` patterns; sentinel path `/storage/trilithon-owner` must be routed to a post-diff ownership-sentinel check that emits `caddy.ownership-sentinel-conflict` rather than being silently ignored.
12. `adopt_running_state` must reject running states that lack the ownership sentinel, or must re-inject the sentinel before persisting the adopted `DesiredState`; a sentinel-free adoption must return `ResolveError::SentinelAbsent`.
13. The `DesiredState.unknown_extensions` field must carry `#[serde(default)]` so existing snapshots from Phases 4–7 deserialise without error; a regression test must deserialise a pre-Phase-8 fixture JSON and assert success.
14. `apply_diff` must specify path-creation semantics for `Added` entries (recursive intermediate node creation with type inference from segment kind: object for string segments, array for integer segments); `pointer_mut` returning `None` on a missing parent must produce `DiffError::MissingParentPath`, not a silent skip.
15. `run_with_shutdown` in `cli/src/run.rs` must construct `Arc<DriftDetector>`, register the drift task in its `JoinSet`, and start it after the sentinel check succeeds and after the capability probe completes; an integration test must assert the task is registered before the `daemon.started` tracing event fires.
16. `regex` and `once_cell` must be added to `[workspace.dependencies]` and to `core/crates/core/Cargo.toml` if regex-based ignore matching is retained; alternatively, `is_caddy_managed` should use a static `&[&str]` prefix table with `str::starts_with` to keep the pure-logic `core` crate free of regex machinery.
17. `Storage::latest_desired_state()` must order by `config_version DESC` (not `created_at_ms`); a `Snapshot::deserialize_desired_state()` method must version-check `canonical_json_version` and return a typed error distinct from `StorageError` so `tick_once` can distinguish "no desired state" from "snapshot undeserializable."
18. The `AuditWriter` write and the `storage.record_drift_event` write inside `record()` must be treated as an atomic unit; either they share a SQLite transaction, or a unique constraint on `(kind, correlation_id)` in `audit_log` must be the backstop with explicit `UNIQUE` violation handling.
19. `TickError` must add a `Serialisation(String)` variant so `canonical_json()` failures propagate correctly without requiring `unwrap()` or nonsemantic error mapping to `DiffError`.
20. The `Arc<tokio::sync::Mutex<()>>` apply mutex must be specified as a Phase 8 exit contract — constructed once in `cli::main`, cloned into `DriftDetector`, and also cloned into the config-apply path (Phase 7/9 applier); the applier must hold the mutex for the duration of every `CaddyClient::load_config` or `patch_config` call.

---

## Unaddressed Findings

Findings that were raised but explicitly accepted as known risk or deferred to a later phase:

| ID | Severity | Finding | Accepted because |
|----|----------|---------|-----------------|
| F009 | MEDIUM | Array index reordering produces inflated `DiffCounts` (e.g., prepending one route shows 3 modified + 1 added instead of 1 added) | Known limitation of scalar-leaf flattening; must be documented in the design notes so operators understand over-counted diffs for route-ordering changes; not a correctness risk for drift detection itself |
| F010 | MEDIUM | `detected_at: i64` (wall-clock) can be non-monotonic under NTP corrections; audit viewer sorted by `detected_at` returns rows out of sequence | Accepted for V1: `detected_at` is documented as "wall-clock for display only"; `correlation_id` (ULID) is the canonical ordering key; requires documentation, not a redesign |
| F011 | MEDIUM | `record()` holding `last_running_hash` mutex across async SQLite write couples API response latency to storage write latency | LOW operational impact in V1 (50ms coupling is acceptable); mitigated by the constraint that the guard covers the write atomically (Constraint 18); full decoupling is a later-phase optimisation |
| F013 | MEDIUM | Skipped ticks (`SkippedApplyInFlight`) produce no audit row; operators cannot distinguish "system was clean" from "drift checks suppressed" | A `config.drift-check-skipped` structured log event (not an audit row) is the V1 mitigation; a formal audit kind is deferred to a later phase when observability requirements are firmer |
| F015 | LOW | Deferred events have no TTL; drift persists indefinitely without re-alerting if operator closes browser without resolving | Deferred-deferral TTL is out of scope for Phase 8; a maximum deferral window and `drift.deferred-timeout` audit kind are Phase 9 scope items |
| F033 | MEDIUM | `canonical_json()` of full running state may exceed the 200 ms `tick_once` budget at large config sizes | The Constraint 4 change (hash over post-redaction diff, not full state) partially mitigates this; full O(diff) hashing is the target design; performance validation against the §13 budget is a Phase 8 acceptance criterion, not a redesign trigger |
| F036 | LOW | `defer_for_manual_reconciliation` has no owner notification path and no TTL escalation | Phase 9 scope: a `drift.deferred` tracing event name and TTL escalation path are planned but out of Phase 8 scope |
| F043 | MEDIUM | ULID timestamp and `detected_at: i64` derived from separate clock reads; discrepancy looks like log tampering to auditors | Accepted for V1: `detected_at` should be derived from `ulid.timestamp_ms() / 1000` (simple one-line fix noted as a constraint) and is low-risk once that alignment is in place |
| F047 | LOW | No post-apply wake-up mechanism; detection gap after a slow apply can be up to (apply_duration + tick_interval) | Accepted as V1 known risk; a `watch` channel signalled by the mutation worker on apply completion is a Phase 9 enhancement |
| F059 | MEDIUM | `last_running_hash` has no `instance_id` dimension; 1:N usage would cross-contaminate dedup | V1 explicitly scopes to `instance_id = "local"` (1:1); multi-instance is future scope; the design must document that `DriftDetector` is single-instance and add a guard against N>1 construction |
| F083 | MEDIUM | Caddy version upgrades add fields to `/config/` response, changing `running_state_hash` on a converged system and producing spurious audit rows | Mitigated by Constraint 4 (hash excludes `unknown_extensions`) and Constraint 13 (`#[serde(default)]`); residual risk from other Caddy-native additions is accepted as a V1 known gap |

---

## Round Summary

| Round | Critical | High | Medium | Low | Top concern |
|-------|----------|------|--------|-----|-------------|
| 1 | 2 | 6 | 5 | 2 | Dedup hash committed on detection, not confirmed resolution — one transient failure creates a permanent blind spot |
| 2 | 3 | 4 | 3 | 1 | Resolution mutation variants missing from enum and carrying no `expected_version` |
| 3 | 3 | 4 | 3 | 1 | `/storage/.*` ignore rule makes ownership-sentinel removal invisible to the drift detector |
| 4 | 3 | 4 | 3 | 1 | `DesiredState` partial-model parse silently drops unmodeled Caddy fields; drift is invisible for anything outside the schema |
| 5 | 4 | 6 | 3 | 1 | `drift_events` table missing from §6 schema and `CaddyClient` trait missing `get_running_config`; Phase 8 is unimplementable as written |
| 6 | 2 | 9 | 2 | 0 | `run(self)` consuming detector is structurally incompatible with Phase 9 `AppState: Arc<DriftDetector>` |
| 7 | 1 | 3 | 1 | 0 | `ObjectKind` missing `Ord` — hard compile blocker for `BTreeMap<ObjectKind, DiffCounts>`; ignore-list filter matches nothing in actual diff path space |
| 8 | 1 | 4 | 1 | 1 | `regex`/`once_cell` absent from workspace manifests — compile blocker for entire diff engine |
| 9 | 1 | 0 | 0 | 0 | Drift detector never wired into `run_with_shutdown` — satisfies all stated acceptance criteria while feature is permanently inert in production |

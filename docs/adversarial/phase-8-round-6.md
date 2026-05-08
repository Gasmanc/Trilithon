# Phase 8 Adversarial Review — Round 6

**Date:** 2026-05-08
**Severity summary:** 2 critical · 9 high · 2 medium · 0 low

---

## New Findings (Round 6)

### F062 — `run(self)` consumes detector; Phase 9 `AppState` requires `Arc<DriftDetector>` [CRITICAL]

**Category:** Composition failures

**Attack:** `run(self)` takes ownership of `DriftDetector` by value. Phase 9 `AppState` declares `pub drift: Arc<crate::drift::DriftDetector>`, requiring the same `DriftDetector` to be shared between the long-running task (spawned with `tokio::spawn`) and the HTTP layer. These two requirements are mutually exclusive. `Arc::try_unwrap` is not safe when the HTTP server holds a clone.

**Scenario:**
1. `cli::main` constructs `DriftDetector` and wraps it in `Arc::new(detector)`.
2. Phase 9 `AppState` is built with `drift: Arc::clone(&detector_arc)`.
3. The drift task needs to call `detector_arc.run(self)` — but `run` takes `self` by value, requiring the sole owner.
4. `Arc::try_unwrap(detector_arc)` fails because `AppState` holds a second clone.
5. The only workaround — not `Arc`-wrapping — means Phase 9 cannot hold `drift: Arc<DriftDetector>`.
6. There is no resolution within the current API surface: the method signature, the Phase 9 `AppState` field type, and the spawn idiom are structurally incompatible.

**Design gap:** `run` must take `self: Arc<DriftDetector>` (or the DriftDetector must be wrapped in a newtype with an interior `Arc`), and `tick_once`, `record`, and `mark_resolved` must all work through shared references. The design must specify exactly which ownership model applies before implementation begins.

---

### F063 — `ResolutionKind` (Phase 8) and `DriftResolution` (storage types) are parallel enums with incompatible vocabularies [HIGH]

**Category:** Semantic drift between layers

**Attack:** `ResolutionKind` (defined in slice 8.6) has three variants: `Adopt`, `Reapply`, `Defer`. `DriftResolution` (in `core::storage::types`) has three variants: `Reapplied`, `Accepted`, `RolledBack`. Neither enum maps cleanly to the other: `Adopt` corresponds loosely to `Accepted` but `DriftResolution::Accepted` is described as "the live state was accepted," while `DriftResolution::RolledBack` has no counterpart in `ResolutionKind`. No conversion is specified anywhere.

**Scenario:**
1. `mark_resolved(correlation_id, ResolutionKind::Adopt)` is called.
2. The implementation must write a `DriftEventRow` with `resolution: Some(DriftResolution::???)`.
3. There is no canonical mapping: `Adopt` could mean `Accepted` or could mean `RolledBack` (if "adopt" implies the running state is made canonical, it is closer to `Accepted`).
4. `ResolutionKind::Defer` has no counterpart at all in `DriftResolution`.
5. The `DriftEventRow` persisted to storage will carry whichever mapping the implementer guesses, silently breaking the Phase 9 dashboard's resolution display.

**Design gap:** One of the two enums must be eliminated and the other used consistently across all layers. The design must define the exact `ResolutionKind` → `DriftEventRow.resolution` conversion or explain why the `DriftEventRow.resolution` field is left `None` for some paths.

---

### F064 — `DriftEventRow` has no `running_state_hash`, `diff_summary`, or `redaction_sites`; `DriftEvent` fields cannot be persisted [HIGH]

**Category:** Semantic drift between layers

**Attack:** `DriftEvent` carries: `running_state_hash: String`, `diff_summary: BTreeMap<ObjectKind, DiffCounts>`, `redacted_diff_json: String`, `redaction_sites: u32`. `DriftEventRow` (the type the storage trait actually accepts) carries: `id`, `correlation_id`, `detected_at`, `snapshot_id`, `diff_json`, `resolution`, `resolved_at`. The field names differ (`diff_json` vs `redacted_diff_json`), and `DriftEventRow` has no `running_state_hash`, no `diff_summary`, and no `redaction_sites` columns.

**Scenario:**
1. `record(event: DriftEvent)` tries to call `self.storage.record_drift_event(event)`.
2. The trait requires `DriftEventRow`, not `DriftEvent`.
3. Implementing a conversion loses `running_state_hash` (the dedup key) and `redaction_sites` (required for transparency reporting in the audit row).
4. The dedup logic reads `event.running_state_hash` from the in-memory guard — the data is available in memory but cannot be recovered from the database after a restart (the restart dedup failure has its own finding; this finding concerns the field-level loss during storage).
5. If `diff_json` in `DriftEventRow` stores the redacted diff, the `running_state_hash` is permanently unrecoverable from the database, breaking any future "was this drift already seen?" query.

**Design gap:** The design must specify the exact `DriftEvent` → `DriftEventRow` mapping, add the missing columns to `DriftEventRow` (and to the underlying database table), or explain which fields are deliberately not persisted and how their absence affects correctness.

---

### F065 — `latest_drift_event` returns the latest row regardless of resolution state; `current` endpoint incorrectly returns resolved drifts [HIGH]

**Category:** Logic flaws

**Attack:** `Storage::latest_drift_event()` returns `Option<DriftEventRow>` with no filter parameter. `DriftEventRow.resolution` can be `Some(...)` (resolved) or `None` (unresolved). Phase 9's `current` handler calls `latest_drift_event()` and returns 204 if `None` — but `None` means no rows exist at all, not "no unresolved rows." If the most recent row has `resolution = Some(Accepted)`, the method returns that resolved row, and the handler serves it to the user as an active drift event.

**Scenario:**
1. Drift is detected; a `DriftEventRow` with `resolution = None` is written.
2. The user resolves the drift; the resolution is recorded.
3. `GET /api/v1/drift/current` calls `latest_drift_event()`.
4. Storage returns the most recent row — the resolved drift event.
5. The handler sees `Some(row)` and returns it as an active problem.
6. The dashboard permanently shows a false drift alert after any resolution.

**Design gap:** The storage trait must expose `latest_unresolved_drift_event()` or accept a resolution filter parameter. Alternatively, the design must specify whether resolution is modelled as an in-place update or an insert of a new row, and derive the query from that choice.

---

### F066 — `mark_resolved` has no mechanism to update `DriftEventRow.resolution` in storage; the field is permanently `None` [HIGH]

**Category:** State machine gaps

**Attack:** `DriftEventRow` carries `resolution: Option<DriftResolution>` and `resolved_at: Option<UnixSeconds>`, implying that resolution is recorded by mutating the stored row. However, the `Storage` trait has no `update_drift_event` or `resolve_drift_event` method. `record_drift_event` is insert-only. `mark_resolved` writes a `config.drift-resolved` audit row and resets `last_running_hash` in memory but has no path to set `DriftEventRow.resolution = Some(...)` in storage.

**Scenario:**
1. Drift detected; `record_drift_event` inserts a row with `resolution = None`.
2. User calls `POST /api/v1/drift/{id}/adopt`; `mark_resolved(id, ResolutionKind::Adopt)` runs.
3. `mark_resolved` writes the `config.drift-resolved` audit row and resets the in-memory hash guard.
4. The `DriftEventRow` in the database still has `resolution = None` and `resolved_at = None`.
5. After daemon restart, `last_running_hash` is `None`. `latest_drift_event()` returns the old row with `resolution = None`, making it appear the original drift was never resolved.

**Design gap:** The `Storage` trait must add a `resolve_drift_event(id: DriftRowId, resolution: DriftResolution, resolved_at: UnixSeconds) -> Result<(), StorageError>` method and `mark_resolved` must call it. The design must specify this write path explicitly.

---

### F067 — `Mutation::ReplaceDesiredState` and `Mutation::DriftDeferred` do not exist in the authoritative `Mutation` enum [CRITICAL]

**Category:** Assumption violation

**Attack:** Slice 8.4 specifies that `adopt_running_state` produces `Mutation::ReplaceDesiredState { new_state, source: ResolveSource::DriftAdopt(...) }` and `defer_for_manual_reconciliation` produces `Mutation::DriftDeferred { event_correlation }`. The authoritative `Mutation` enum has 13 variants: `CreateRoute`, `UpdateRoute`, `DeleteRoute`, `CreateUpstream`, `UpdateUpstream`, `DeleteUpstream`, `AttachPolicy`, `DetachPolicy`, `UpgradePolicy`, `SetGlobalConfig`, `SetTlsConfig`, `ImportFromCaddyfile`, `Rollback`. Neither `ReplaceDesiredState` nor `DriftDeferred` exists. `ResolveSource` also does not exist.

**Scenario:**
1. `adopt_running_state` constructs `Mutation::ReplaceDesiredState { ... }`.
2. The variant does not exist; the code fails to compile.
3. Even if the implementer adds the variants, the `expected_version` field that every existing variant carries is absent from the new variants.
4. Every `exhaustive match` in `Mutation::expected_version()` and `Mutation::kind()` will not compile until both new variants are handled.
5. Every downstream consumer of `Mutation` (apply pipeline, validator, audit writer, JSON serialisation) must be updated. The design does not mention any of these updates.

**Design gap:** Either `Mutation` must be extended with `ReplaceDesiredState`, `DriftDeferred`, and `ResolveSource` in the same commit (with migration of all match sites), or the adoption path must reuse an existing variant. The design must specify which approach before implementation.

---

### F068 — `SchemaRegistry` is not a field of `DriftDetector`; `redact_diff` cannot be called in `tick_once` [HIGH]

**Category:** Composition failures

**Attack:** `tick_once` step 6 calls `self.diff_engine.redact_diff(diff, schema)` where `schema: &SchemaRegistry`. `DriftDetector`'s struct declaration has no `schema: Arc<SchemaRegistry>` field. `SchemaRegistry` is required by both `DiffEngine::redact_diff` and `SecretsVault::redact`. The design states Phase 6 ships the redactor but does not wire `SchemaRegistry` into `DriftDetector`.

**Scenario:**
1. `tick_once` reaches step 6 and needs to produce `redacted_diff_json`.
2. There is no `self.schema` field to pass to `redact_diff`.
3. Path of least resistance for an implementer: pass a dummy `SchemaRegistry` that skips all redaction, silently leaking plaintext secrets into audit rows.
4. ADR-0009 makes audit rows immutable. The leaked secrets are permanently in the audit log.

**Design gap:** The design must add `pub schema: Arc<SchemaRegistry>` to `DriftDetector` and show the wiring in `cli::main`. This field is load-bearing for the redaction guarantee.

---

### F069 — `record` acquires `last_running_hash` lock before two fallible async writes; partial failure creates duplicate immutable audit rows [HIGH]

**Category:** Data race / interleaving

**Attack:** `record(event)` holds the `last_running_hash` `MutexGuard` across (a) an `AuditWriter::record` write and (b) a `storage.record_drift_event` write. If step (a) succeeds and step (b) fails, the guard is dropped without setting the hash. The next tick re-enters with `*guard == None`, passes the dedup check, and calls `AuditWriter::record` a second time — writing a second `config.drift-detected` row for the same event.

**Scenario:**
1. `record(event_A)` begins; guard acquired; `*guard == None`.
2. `AuditWriter::record` writes one `config.drift-detected` row.
3. `storage.record_drift_event` returns `StorageError::SqliteBusy`.
4. `record` returns `Err`; guard dropped; `*guard` still `None`.
5. Next tick: same drift, same hash; dedup passes; second `config.drift-detected` audit row written.
6. Two immutable rows exist for one detection event. ADR-0009: neither can be removed.

**Design gap:** The audit write and storage write must be treated as a logical unit, or a dedup constraint on `(kind, correlation_id)` must be added to `audit_log` as a backstop. The design must specify which.

---

### F070 — `latest_desired_state()` returns `Snapshot`, not `DesiredState`; conversion to `DesiredState` is unspecified [HIGH]

**Category:** Assumption violation

**Attack:** `Storage::latest_desired_state()` returns `Result<Option<Snapshot>, StorageError>`. `tick_once` step 3 uses the result as `desired: DesiredState` in `structural_diff(&desired, &running)`. There is no `From<Snapshot> for DesiredState` implementation and no conversion utility. `Snapshot.desired_state_json` is a raw JSON `TEXT` column; deserialising it requires knowing `canonical_json_version`. The error path cannot distinguish "no desired state" (first run) from "desired state undeserializable" (data corruption).

**Scenario:**
1. `tick_once` step 3: `storage.latest_desired_state()` returns `Some(snapshot)`.
2. The implementer writes `serde_json::from_str::<DesiredState>(&snapshot.desired_state_json)?`.
3. A snapshot written by a future version with schema V2 fails deserialization; `TickError::Storage("...")` is returned.
4. The drift loop logs a warning and continues, permanently unable to compare against the current desired state. No audit row. Drift detection silently broken.

**Design gap:** The design must specify a `Snapshot::deserialize_desired_state(&self) -> Result<DesiredState, SnapshotError>` method that version-checks `canonical_json_version`. `TickError` must carry a `SnapshotDeserialize` variant distinct from `Storage` so the caller can route appropriately.

---

### F071 — Running config → `DesiredState` conversion includes Caddy-native fields not in stored desired state; permanent phantom drift on every tick [HIGH]

**Category:** Logic flaws

**Attack:** `tick_once` step 2 converts `CaddyConfig` → `DesiredState` with unknown fields landing in `unknown_extensions`. Caddy's full running config includes admin listeners, logging config, metrics config, and tracing config — fields Trilithon never models. The stored desired state has empty `unknown_extensions`. `structural_diff(desired, running)` sees all `unknown_extensions` entries as `Added`, producing a non-empty diff for a fully converged system.

**Scenario:**
1. System is fully converged: stored desired state matches Caddy exactly (for Trilithon-modelled fields).
2. `tick_once` ingests full Caddy config; `admin` and `logging` land in `unknown_extensions`.
3. `structural_diff` classifies all `unknown_extensions` paths as `Added`.
4. Every tick returns `Drifted`. A `config.drift-detected` row is written on the first tick.
5. Dedup suppresses subsequent rows (same hash), but the system is permanently in "drifted" state.
6. No resolution path closes this loop: `reapply_desired_state` re-applies the stored state (no change), `adopt_running_state` would pull in all Caddy-native fields permanently.

**Design gap:** The design must specify whether `unknown_extensions` on the running side are excluded from the diff, mapped to a separate "unmanaged" diff category, or compared symmetrically. This boundary must be explicit before any diffing code is written.

---

### F072 — `apply_mutex.try_lock()` result is a temporary; mutex released before fetch begins [HIGH]

**Category:** Data race / interleaving

**Attack:** If `tick_once` step 1 is written as `if self.apply_mutex.try_lock().is_err() { return ... }`, the `MutexGuard` is a temporary dropped at the end of the `if` expression — before the function body continues. The mutex is unlocked immediately after the check. An apply can start between the `try_lock` call and `get_running_config()`.

**Scenario:**
1. `tick_once` calls `try_lock()`. Succeeds; temporary guard created and immediately dropped.
2. Applier acquires the mutex; begins loading a new config.
3. `tick_once` proceeds to `get_running_config()` and captures a partially-applied Caddy config.
4. `structural_diff` produces entries from a transient mid-apply state.
5. A spurious drift event is written with a transient hash. Dedup records this hash; the next tick's pre-apply hash is treated as a duplicate and silently suppressed.

**Design gap:** The design must explicitly state that the `MutexGuard` is bound to a named variable (`let _guard = ...`) that lives for the duration of the fetch-and-diff pipeline. This is load-bearing, not a style choice.

---

### F073 — `DriftDetectorConfig.interval: Duration` has no TOML deserializer; config loading path is unspecified [MEDIUM]

**Category:** Boundary condition exploits

**Attack:** `Duration` does not implement `serde::Deserialize` from a TOML integer. Architecture §7.2 refers to `drift_interval` as a configurable integer in seconds. If the config loader tries to deserialise `DriftDetectorConfig` directly from TOML, startup fails because `Duration` is not a TOML primitive. If the field is read as `u64` then converted, a separate intermediate struct is needed but none is specified.

**Scenario:**
1. User sets `drift_check_interval_seconds = 30` in `trilithon.toml`.
2. Config loader deserialises `DriftDetectorConfig { interval: Duration }`.
3. Deserialisation fails; daemon either panics at startup or silently uses `Default` (60 s).
4. In the silent-default case, the user's configured interval is ignored with no warning. The interval floor enforcement (F037) is also bypassed.

**Design gap:** The design must specify either a `#[serde(deserialize_with = ...)]` wrapper that reads `u64` seconds, or a separate `DriftDetectorConfigRaw` intermediate struct with explicit conversion. The conversion must validate against the [10, 3600] range.

---

### F074 — `record` and `mark_resolved` are `pub`; any code with `Arc<DriftDetector>` can fabricate resolutions [MEDIUM]

**Category:** Trust boundary violations

**Attack:** `DriftDetector.record` and `mark_resolved` are `pub`. Phase 9 stores `Arc<DriftDetector>` in `AppState`. Any handler can call `state.drift.mark_resolved(any_ulid, any_kind)` directly, bypassing the authentication check, the event-existence check (404 if absent), and the mutation pipeline. A call with a fabricated `correlation_id` writes a `config.drift-resolved` row for a drift event that never existed and resets `last_running_hash` to `None`, causing the dedup gate to allow a duplicate detection row on the next tick.

**Scenario:**
1. A bug in a Phase 9 handler passes the wrong `correlation_id` to `mark_resolved`.
2. A `config.drift-resolved` row is written for a ULID with no corresponding `config.drift-detected` row.
3. The audit trail shows a resolution without a preceding detection — permanently inconsistent.
4. `last_running_hash` is reset to `None`; the next tick unconditionally writes a new detection row regardless of drift state.

**Design gap:** `record` and `mark_resolved` should not be `pub`; they should be gated behind a trait that the HTTP handlers receive instead of `Arc<DriftDetector>`. At minimum, `mark_resolved` must validate that a `config.drift-detected` row exists for the given `correlation_id` before writing a resolution.

---

## Summary

**Critical:** 2 (F062, F067)
**High:** 9 (F063, F064, F065, F066, F068, F069, F070, F071, F072)
**Medium:** 2 (F073, F074)
**Low:** 0

**Top concern:** F067 and F062 are jointly the most dangerous — the three resolver APIs produce `Mutation` variants that do not exist in the authoritative enum, and `run(self)` consumes `DriftDetector` in a way that is structurally incompatible with Phase 9's `AppState: Arc<DriftDetector>`. Neither can be patched silently: both require explicit design changes before any code compiles.

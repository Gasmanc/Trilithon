# Phase 8 Adversarial Review — Round 8

**Date:** 2026-05-08
**Severity summary:** 1 critical · 4 high · 1 medium · 1 low

---

## New Findings (Round 8)

### F080 — `regex` and `once_cell` crates absent from workspace manifests; slice 8.2 is a compile blocker [CRITICAL]

**Category:** Logic flaws

**Attack:** Slice 8.2 specifies `once_cell::sync::Lazy<Vec<regex::Regex>>` for compiling `CADDY_MANAGED_PATH_PATTERNS` at startup. Neither `regex` nor `once_cell` appears in the workspace `Cargo.toml` or in `core/crates/core/Cargo.toml`. Any attempt to write `use once_cell::sync::Lazy;` or `use regex::Regex;` in `diff/ignore_list.rs` fails immediately with `E0432: unresolved import`.

**Scenario:**
1. Implementer creates `core/crates/core/src/diff/ignore_list.rs` as specified.
2. `use once_cell::sync::Lazy;` — compiler error: no external crate `once_cell`.
3. `use regex::Regex;` — compiler error: no external crate `regex`.
4. `just check` fails. Slice 8.2 cannot compile.
5. Since `structural_diff` (slice 8.1) is specified to call `is_caddy_managed`, all of slice 8.1 is also blocked.

**Design gap:** The design must either: (a) add `regex` and `once_cell` to `[workspace.dependencies]` and to `[dependencies]` in `core/crates/core/Cargo.toml`; or (b) rewrite `is_caddy_managed` using a static `&[&str]` prefix table and `str::starts_with`, which avoids pulling regex machinery into the pure-logic `core` crate. Option (b) is architecturally cleaner since all patterns are anchored at `^` with known prefixes.

---

### F081 — `tick_once` builds `DriftEvent`; `Storage::record_drift_event` takes `DriftEventRow`; no conversion is specified [HIGH]

**Category:** Logic flaws

**Attack:** `tick_once` step 6 constructs `DriftEvent { before_snapshot_id, running_state_hash, diff_summary, detected_at, correlation_id, redacted_diff_json, redaction_sites }`. Slice 8.6 step 4 calls `self.storage.record_drift_event(event).await?`. But `Storage::record_drift_event` takes `DriftEventRow`, which has fields: `id: DriftRowId`, `correlation_id: String`, `detected_at: UnixSeconds`, `snapshot_id: SnapshotId`, `diff_json: String`, `resolution: Option<DriftResolution>`, `resolved_at: Option<UnixSeconds>`. `DriftEventRow` has no `running_state_hash`, no `diff_summary`, no `redacted_diff_json`, and no `redaction_sites`. `DriftEvent` has no `id`, no `resolution`, and no `resolved_at`. These are incompatible types; passing `event: DriftEvent` to a function expecting `DriftEventRow` is a compile error.

**Scenario:**
1. `tick_once` returns `Ok(Drifted { event: DriftEvent { ... } })`.
2. `run` calls `self.record(event).await?`.
3. `record` calls `self.storage.record_drift_event(event).await?` — compile error: expected `DriftEventRow`, found `DriftEvent`.
4. No drift event is ever persisted to the database.
5. Phase 9's resolution endpoints (which look up drift events by `correlation_id`) always return 404 because no rows exist.

**Design gap:** The design must specify the `DriftEvent` → `DriftEventRow` conversion: which fields map to which, how `running_state_hash` / `diff_summary` / `redacted_diff_json` / `redaction_sites` are carried (added to `DriftEventRow`, serialized into `diff_json`, or stored separately), and where `DriftRowId` is assigned. This specification must ship in the same commit as the missing `drift_events` schema migration (F048).

---

### F082 — `apply_diff` for `Added` entries uses `pointer_mut` which returns `None` on missing parent paths; no path-creation semantics specified [HIGH]

**Category:** Logic flaws

**Attack:** `apply_diff` is specified as: "walk the entries, mutate the `state.canonical_json()` value at each pointer, and reparse." For `DiffEntry::Added { path, after }`, the implementation calls `value.pointer_mut(path)` — but `serde_json::Value::pointer_mut` returns `None` if any intermediate node is absent. For an `Added` entry, the parent path by definition did not exist in the `before` state. `pointer_mut` always returns `None` for new paths. The mutation is silently skipped. The algorithm produces no error — it silently drops `Added` entries.

**Scenario:**
1. `state_a` has no `/routes/new-route`. `state_b` has one.
2. `structural_diff(state_a, state_b)` produces `Added { path: "/routes/new-route/upstreams/0/dial", after: "127.0.0.1:8080" }`.
3. `apply_diff(state_a, diff)` parses `canonical_json(state_a)` to a `serde_json::Value`.
4. `value.pointer_mut("/routes/new-route/upstreams/0/dial")` → `None`.
5. The mutation is silently skipped. The final `DesiredState` has no `new-route`.
6. `apply_diff(state_a, diff(state_a, state_b)) != state_b`. The `apply_diff_inverse_round_trip` acceptance test fails.
7. `reapply_desired_state` resolution also fails silently: out-of-band route additions are lost during resolution.

**Design gap:** The design must specify path-creation semantics for `apply_diff` on `Added` entries: whether intermediate nodes are created recursively (with what type — object for string segments, array for integer segments), and what error is returned when an intermediate node has an incompatible type. A new `DiffError::MissingParentPath { path: JsonPointer }` variant may be required.

---

### F083 — `running_state_hash` covers `unknown_extensions`; Caddy version upgrades change the hash on a converged system, producing spurious drift audit rows [MEDIUM]

**Category:** Eventual consistency

**Attack:** `running_state_hash = SHA-256(canonical_json(running))` covers the full `DesiredState` including `unknown_extensions`. When Caddy is upgraded and the new version adds fields to the `/config/` response, those fields land in `unknown_extensions`. The hash changes. The dedup guard sees a new hash → `record()` writes a new `config.drift-detected` row even though no route/upstream/TLS configuration changed.

**Scenario:**
1. System is converged. `last_running_hash` = hash-of-Caddy-2.8-response.
2. Caddy is upgraded to 2.9, which adds `"srv_name"` to the `/config/` response.
3. `running.unknown_extensions` gains `JsonPointer("/srv_name") -> "default"`.
4. New `canonical_json(running)` hash differs from stored hash.
5. Dedup check passes. `record()` writes a `config.drift-detected` audit row for a fully converged system.
6. The operator resolves the alarm; the next Caddy patch release triggers it again.

**Design gap:** `running_state_hash` must be computed from a projection of `DesiredState` that excludes `unknown_extensions`, using a dedicated `DesiredState::without_unknown_extensions()` method. The design must specify this explicitly, including whether the projection also strips Trilithon metadata fields (`config_version`, `instance_id`, `created_at`, `trilithon_version`) for the same reason (F058/F077 interact here).

---

### F084 — `DesiredState::unknown_extensions` added without `#[serde(default)]`; Phase 8 deployment fails to deserialize pre-Phase-8 snapshots [HIGH]

**Category:** State machine gaps

**Attack:** `DesiredState` gains `pub unknown_extensions: BTreeMap<JsonPointer, serde_json::Value>` in Phase 8. Every existing snapshot was serialized by Phases 4–7 without this field. When Phase 8's code calls `serde_json::from_str::<DesiredState>(&snapshot.desired_state_json)`, serde's derived `Deserialize` impl sees a missing `unknown_extensions` key and returns `Error("missing field 'unknown_extensions'")` — unless `#[serde(default)]` is specified on the field.

**Scenario:**
1. Phase 8 is deployed to a production system running since Phase 5.
2. SQLite contains 47 snapshots, none with `"unknown_extensions"`.
3. `tick_once` step 3 calls `self.storage.latest_desired_state().await?`.
4. `SqliteStorage::latest_desired_state` reads the row; `serde_json::from_str::<DesiredState>(...)` fails.
5. `StorageError::Deser(...)` returned; `tick_once` returns `Err(TickError::Storage(...))`.
6. Every tick fails. Drift detection is permanently broken until the database is manually patched.

**Design gap:** The `unknown_extensions` field must be annotated with `#[serde(default)]` so existing snapshots without the key deserialize to `BTreeMap::new()`. The Phase 8 commit must include a test that deserializes a Phase 4–style JSON fixture (without `"unknown_extensions"`) into `DesiredState` and asserts it succeeds.

---

### F085 — `DefaultDiffEngine` is a unit struct; `redact_diff` requires `SchemaRegistry`, forcing `core::diff` to import `core::schema`; transitive dependency may violate the three-layer invariant [HIGH]

**Category:** Missing invariant enforcement

**Attack:** `DefaultDiffEngine` is declared as a unit struct with no fields. The `DiffEngine` trait requires `fn redact_diff(&self, diff: &Diff, schema: &SchemaRegistry) -> Result<RedactedDiff, DiffError>`. For `DefaultDiffEngine` to implement this, `core::diff` must import `SchemaRegistry`. `SchemaRegistry` is Phase 6 infrastructure. If `SchemaRegistry` (or its transitive dependencies) imports any `tokio`, async I/O, or storage type, then `core::diff` → `core::schema` → `<I/O type>` breaks the three-layer rule: `core/` must have no I/O dependencies.

**Scenario:**
1. Implementer writes `DefaultDiffEngine::redact_diff` which calls `schema.is_secret_path(path)`.
2. `SchemaRegistry` in `core::schema` depends on `core::storage::types` for `SecretFieldDescriptor`.
3. `core::storage::types` imports `sqlx::Type` (a storage-layer type).
4. `core/crates/core/Cargo.toml` now transitively depends on `sqlx` — a storage adapter.
5. `cargo deny check` or the three-layer manifest check flags the dependency.
6. Even if the build succeeds, future phases that need to swap the redaction strategy must modify `DefaultDiffEngine` rather than injecting a replacement.

**Design gap:** The design must specify either: (a) add `pub redactor: Arc<dyn Redactor>` to `DefaultDiffEngine` (eliminating the unit struct) where `Redactor` is a new core trait with `fn is_secret_path(&self, path: &JsonPointer) -> bool`; or (b) split `redact_diff` out of the `DiffEngine` trait entirely into a separate `RedactionEngine` trait. The current design conflates structural diffing (pure path comparison) with secrets redaction (policy-based field masking) in a single trait, violating single responsibility.

---

### F086 — `canonical_json()` can fail; `TickError` has no serialization variant; hash computation failure causes `unwrap()` or incorrect error mapping [LOW]

**Category:** Logic flaws

**Attack:** `tick_once` step 5 computes `SHA-256(canonical_json(running))`. `canonical_json()` returns `Result<String, serde_json::Error>`. `TickError` has three variants: `CaddyFetch(String)`, `Storage(String)`, `Diff(#[from] DiffError)`. None covers `serde_json::Error`. An implementer who writes `canonical_json(running)?` gets a compile error. One who writes `.unwrap()` violates the workspace lint `unwrap_used = "deny"`. One who maps to `TickError::Diff(...)` produces a semantically wrong error (this is not a diff failure).

**Scenario:**
1. A Caddy plugin writes a JSON number with a non-finite float into the config response.
2. `canonical_json(running)` encounters the non-serializable value; returns `Err(serde_json::Error {...})`.
3. No valid `TickError` variant exists to carry this error.
4. Implementer resorts to `unwrap()` or a nonsemantic mapping.
5. Either the daemon panics in the tokio task (drift loop silently dies, variant of F050) or the error is misclassified as a `DiffError`, confusing diagnosis.

**Design gap:** The design must add `#[error("serialisation: {0}")] Serialisation(String)` to `TickError` and specify that `canonical_json` failures in step 5 are propagated through this variant. The acceptance criteria must include a test that injects a non-serializable value into `unknown_extensions` and asserts `tick_once` returns `Err(TickError::Serialisation(...))` rather than panicking.

---

## Summary

**Critical:** 1 (F080)
**High:** 4 (F081, F082, F084, F085)
**Medium:** 1 (F083)
**Low:** 1 (F086)

**Top concern:** F080 is a hard compile blocker — `regex` and `once_cell` are absent from the workspace manifests so `is_caddy_managed` cannot be built and the entire diff engine is blocked. F081 compounds this: even with a working diff engine, `tick_once` constructs `DriftEvent` but the storage layer only accepts `DriftEventRow` — an incompatible type — so no drift event can ever be persisted.

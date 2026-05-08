# Phase 8 Adversarial Review — Round 7

**Date:** 2026-05-08
**Severity summary:** 1 critical · 3 high · 1 medium · 0 low

---

## New Findings (Round 7)

### F075 — `ObjectKind` missing `Ord`; `BTreeMap<ObjectKind, DiffCounts>` fails to compile [CRITICAL]

**Category:** Logic flaws

**Attack:** `DriftEvent.diff_summary` is declared as `BTreeMap<ObjectKind, DiffCounts>`. `BTreeMap` requires `K: Ord`. The design derives `Hash` and `Eq` on `ObjectKind` but does NOT derive `Ord` or `PartialOrd`. Any code that constructs or inserts into `BTreeMap<ObjectKind, DiffCounts>` fails to compile with `E0277: the trait bound ObjectKind: Ord is not satisfied`.

**Scenario:**
1. `DriftEvent` is constructed in `tick_once` step 6.
2. The `diff_summary: BTreeMap<ObjectKind, DiffCounts>` field requires `BTreeMap::insert`, which requires `ObjectKind: Ord`.
3. The compiler emits `E0277`. The code does not compile.
4. `DriftEvent` cannot be constructed. `TickOutcome::Drifted { event }` is unreachable. Every acceptance test that exercises the drift path is blocked.
5. The `DiffCounts`-by-`ObjectKind` classification step in slice 8.3 cannot be implemented until `Ord` is derived.

**Design gap:** `ObjectKind` must derive `Ord` and `PartialOrd`. Because `ObjectKind` is a unit enum, the derived ordering is declaration order (`Route < Upstream < Tls < Server < Policy < Other`) — deterministic and appropriate. Alternatively, `HashMap<ObjectKind, DiffCounts>` can be used if ordering is not required for serialization stability, but this must be specified because `serde_json` serializes `HashMap` in non-deterministic insertion order.

---

### F076 — `unknown_extensions` keys serialize as nested JSON-object keys; flatten produces double-encoded pointer paths that never match [HIGH]

**Category:** Logic flaws

**Attack:** `structural_diff` flattens `state_a.canonical_json()`. `DesiredState.unknown_extensions: BTreeMap<JsonPointer, serde_json::Value>` serializes as a JSON object whose keys are the `JsonPointer` strings themselves (e.g., `"/apps/http/servers/srv0/routes"`). The flattener descends into `unknown_extensions` as an object key, then treats the key string as a path segment — escaping `/` to `~1` per JSON pointer spec. The emitted path is `/unknown_extensions/~1apps~1http~1servers~1srv0~1routes`, not `/apps/http/servers/srv0/routes`. The stored desired state has empty `unknown_extensions`; the running-state `DesiredState` has populated ones. The diff emits paths that: (a) never match `is_caddy_managed` regexes (they start with `/unknown_extensions/`, not `/apps/`); and (b) cannot be used for resolution (the path does not correspond to any real Caddy JSON location).

**Scenario:**
1. Caddy has `{"admin": {...}, "apps": {...}}` in its running config.
2. `tick_once` ingests the full config; `"admin"` lands in `unknown_extensions` with key `JsonPointer("/admin")`.
3. `flatten(canonical_json(running))` produces `/unknown_extensions/~1admin/...`.
4. `structural_diff` emits `Added { path: "/unknown_extensions/~1admin", ... }`.
5. `is_caddy_managed("/unknown_extensions/~1admin")` → false. Entry survives the filter.
6. Every tick returns `Drifted` for a path that is garbage. `adopt_running_state` stores a `DesiredState` with `unknown_extensions` containing `JsonPointer("/admin")` as a key, which corrupts the next render's output.

**Design gap:** `unknown_extensions` must not be round-tripped through `canonical_json()` for the purposes of diffing. Either: (a) strip `unknown_extensions` from both sides before flattening and diff them separately using their raw string keys as pre-formed path segments; or (b) expand `unknown_extensions` inline during the flatten pass, treating the `JsonPointer` key as a complete path prefix rather than a new segment to escape.

---

### F077 — `CADDY_MANAGED_PATH_PATTERNS` targets Caddy-JSON path space; `structural_diff` flattens `DesiredState` path space; the filter matches nothing [HIGH]

**Category:** Logic flaws

**Attack:** `structural_diff` flattens `DesiredState` canonical JSON. The resulting flat map has paths in the `DesiredState` schema: `/version`, `/routes/{id}/handle`, `/tls/automation/policies`. `CADDY_MANAGED_PATH_PATTERNS` contains regexes for Caddy-JSON paths: `^/apps/tls/automation/policies/...`, `^/storage/.*`, `^/apps/http/servers/.../request_id$`. None of these match `DesiredState`-schema paths (which do not start with `/apps/`). The filter matches nothing in the flat map. Conversely, the Trilithon metadata fields in `DesiredState` (`/version`, route timestamps) pass through unfiltered and appear as `Modified` diff entries on every tick.

**Scenario:**
1. Stored desired state has `version = 42`. Running-state-as-`DesiredState` has `version = 0`.
2. `flatten(canonical_json(stored))` produces `/version = 42`.
3. `flatten(canonical_json(running))` produces `/version = 0`.
4. `is_caddy_managed("/version")` → false (no pattern matches).
5. `structural_diff` emits `Modified { path: "/version", before: 42, after: 0 }`.
6. Every tick returns `Drifted`. The dedup hash suppresses repeated rows, but the system is permanently in drifted state with no clean path to `Clean`.

**Design gap:** The ignore-list patterns were written for Caddy-JSON path space but the diff operates in `DesiredState` path space. The design must either: (a) define a separate set of `DESIRED_STATE_METADATA_PATTERNS` that covers Trilithon-specific fields to be excluded from the diff; or (b) strip metadata fields from `DesiredState` before flattening, using a projection that retains only route/upstream/TLS/global config. Option (b) is safer because it does not require maintaining a parallel schema list.

---

### F078 — `record()` passes pre-redacted string to `AuditWriter`; writer re-runs redactor on already-redacted data; `redaction_sites` stored as `0` [HIGH]

**Category:** Semantic drift between layers

**Attack:** `record()` step 3 calls `AuditWriter` with `redacted_diff_json = Some(event.redacted_diff_json)` — a pre-redacted string produced by `DiffEngine::redact_diff` in `tick_once`. Phase 6's `AuditWriter` accepts an `AuditAppend.diff: Option<serde_json::Value>` (unredacted) and internally runs the redactor, capturing `(redacted_value, sites_count)`. Two incompatibilities:

1. Field name mismatch: `AuditAppend` has no `redacted_diff_json` field. The design names a parameter that does not exist.
2. If an implementer corrects this by passing the pre-redacted string as `diff: Some(serde_json::from_str(&event.redacted_diff_json)?)`, the `AuditWriter` runs the redactor again on already-redacted data. The redactor finds no secrets (all are `[REDACTED]` tokens) and returns `sites = 0`. The stored `audit_log.redaction_sites` is `0`, while the actual redaction count was `N > 0`.

**Scenario:**
1. Drift detected with 3 secret fields. `DiffEngine::redact_diff` produces `event.redacted_diff_json` with `redaction_sites = 3`.
2. `record()` passes the pre-redacted diff to `AuditWriter::record`.
3. `AuditWriter` re-runs the redactor; finds 0 new secrets; stores `redaction_sites = 0`.
4. The audit row permanently claims no secrets were redacted. Per ADR-0009, the row cannot be corrected.
5. A compliance audit shows `redaction_sites = 0` for all `config.drift-detected` rows, falsely certifying that no sensitive data was present in any drift event.

**Design gap:** Phase 8's `record()` must pass the raw unredacted `Diff` object as `AuditAppend.diff`, not the pre-redacted string. The `DriftEvent.redacted_diff_json` field should be built from the audit writer's output (the stored row's `redacted_diff_json`), not computed independently before the writer call. The design must cross-reference Phase 6's `AuditWriter::record` signature to confirm field names and ownership of the redaction step.

---

### F079 — `apply_mutex` acquirer is unspecified in Phase 8; the mutex is structurally inert until Phase 9 wires the applier [MEDIUM]

**Category:** Missing invariant enforcement

**Attack:** `DriftDetector.apply_mutex: Arc<tokio::sync::Mutex<()>>` is documented as "shared with the applier." `tick_once` step 1 calls `try_lock()`; if it fails, `SkippedApplyInFlight` is returned. Phase 8 does not specify: (a) which module is the "applier," (b) where in the apply pipeline the applier acquires this mutex, or (c) how the `Arc` clone reaches the applier. From Phase 8's perspective, nothing holds the mutex on the other side. `try_lock` always succeeds. The protection is absent.

**Scenario:**
1. Phase 8 is implemented and tested in isolation. `try_lock` always succeeds (no one acquires the mutex from the other side).
2. The acceptance test `drift_skip_when_apply_in_flight` manually holds the mutex. All tests pass.
3. Integration with Phase 9's apply path proceeds. If Phase 9 forgets to acquire the mutex (because Phase 8 did not specify the requirement), `tick_once` captures mid-apply Caddy state.
4. A spurious drift event is written with a transient hash. The dedup guard records this hash.
5. When the apply completes and Caddy converges, the hash matches the recorded transient hash. The dedup check suppresses the first post-apply tick. Real drift introduced at the same time as the apply is permanently silently suppressed.

**Design gap:** Phase 8 must specify, as an exit condition: "The `Arc<tokio::sync::Mutex<()>>` is constructed once in `cli::main`, cloned into `DriftDetector`, and also cloned into the config-apply path (Phase 7/9 applier). The applier holds the mutex for the duration of every `CaddyClient::load_config` or `patch_config` call." This must be an explicit Phase 8 contract, not a Phase 9 assumption.

---

## Summary

**Critical:** 1 (F075)
**High:** 3 (F076, F077, F078)
**Medium:** 1 (F079)
**Low:** 0

**Top concern:** F075 is a hard compile blocker — `BTreeMap<ObjectKind, DiffCounts>` requires `Ord`, and without it no drift event can be constructed, `tick_once` cannot run, and no acceptance test can execute. F077 and F076 together mean the ignore-list filter matches nothing in the actual path space the diff engine operates in — every tick on a real system produces phantom drift entries from Trilithon metadata fields and double-encoded unknown-extension paths.

# Adversarial Review — Phase 7 — Round 4

**Prior rounds:** R1 (F001–F010), R2 (F011–F019), R3 (F020–F026). All unaddressed.

---

## Summary

Round 4 probed five surfaces prior rounds left untouched: hostname exposure in the `ApplyOutcome` return value, the startup ordering gap between the migration runner and the apply-lock table, the unvalidated `instance_id` field, the silently dead `ReloadKind::Abrupt` variant, and the semantic divergence between `snapshot_id` (hash of Trilithon canonical JSON) and the Caddy JSON actually sent to `POST /load`. The diminishing finding rate signals the design has been thoroughly probed; Round 5 is unlikely to surface material new issues.

---

## Findings

### F027 — `AppliedState::TlsIssuing { hostnames: Vec<String> }` leaks managed domain names into tracing logs via derived `Debug`
**Severity:** MEDIUM
**Category:** data-exposure
**Slice:** 7.2 / 7.8

**Attack:** A route with managed TLS for `["api.internal.corp.example.com", "www.customer-name.io"]` is applied successfully. The caller emits `tracing::info!(outcome = ?apply_outcome)`. The derived `Debug` on `AppliedState::TlsIssuing` includes the full hostname list verbatim. If the log backend is a third-party aggregator, the managed domain list becomes an enumerable asset with no redaction. On multi-tenant deployments this is cross-tenant data disclosure.

**Why the design doesn't handle it:** `hostnames: Vec<String>` has a derived `Debug`. The `RedactedDiff` newtype guards the audit diff column but applies no discipline to the `ApplyOutcome` return value, which escapes the audit path into whatever logging the caller applies.

**Blast radius:** Domain names for every site with managed TLS are logged at INFO level on every apply. Operators do not expect hostname enumerability from an apply-success log event.

**Recommended mitigation:** Introduce `SensitiveHostnames(Vec<String>)` with a `Debug`/`Display` impl that emits only the count (`"<N hostnames redacted>"`). Use this in `AppliedState::TlsIssuing`. Expose a non-Debug accessor for callers that legitimately need the list (the TLS observer already receives them at construction time).

---

### F028 — `apply_locks` table may not exist when `CaddyApplier::apply` first runs; lock silently not acquired and apply proceeds unguarded
**Severity:** HIGH
**Category:** assumption-violation
**Slice:** 7.5 / 7.6

**Attack:** `SqliteStorage::open()` opens the pool but does not run migrations. Migrations are run by a separate `apply_migrations()` call in `run.rs`. Nothing in the design enforces that `CaddyApplier` cannot be constructed and called before `apply_migrations()` completes. If a test harness or a startup ordering regression constructs `CaddyApplier` directly from a `SqlitePool` without going through the migration gate, `INSERT INTO apply_locks` executes against a schema that lacks the table. The result is a `sqlx::Error::Database("no such table: apply_locks")` mapped to `ApplyError::Storage(String)`. The lock is never acquired; the apply proceeds unguarded.

**Why the design doesn't handle it:** The ordering invariant "migrations run before any apply" is enforced by convention in `run.rs`, not by the type system. There is no "ready" state on `CaddyApplier` that proves migrations completed.

**Blast radius:** Two daemon processes both fail to insert into `apply_locks`, both proceed unguarded to `POST /load`. Caddy receives two overlapping calls; whichever lands last wins. The earlier caller's version-advance commit then fails, leaving a `config.applied` audit row for a config Caddy is not running.

**Recommended mitigation:** Add a startup precondition check in `CaddyApplier::new` (or an explicit `assert_schema_ready` call `run.rs` must invoke before creating the applier) that verifies `apply_locks` exists via `SELECT name FROM sqlite_master WHERE type='table' AND name='apply_locks'`. Return a typed `ApplyError::SchemaMissing` on failure, converting a runtime silent-skip into a startup failure.

---

### F029 — `instance_id: String` has no validation; empty or malformed values corrupt the lock key and sentinel
**Severity:** MEDIUM
**Category:** assumption-violation
**Slice:** 7.4 / 7.6

**Attack:** A caller constructs `CaddyApplier { instance_id: String::new(), ... }`. The renderer inserts `"@id": "trilithon-owner-"` (no discriminating suffix). A second instance with `instance_id = "local"` writes `"@id": "trilithon-owner-local"`. The two `@id` values differ, so neither instance detects the other's sentinel. Simultaneously, `INSERT INTO apply_locks (instance_id = "")` and `INSERT INTO apply_locks (instance_id = "local")` are different rows — the lock table does not protect against the two-instance scenario because the keys differ.

**Why the design doesn't handle it:** `instance_id` is a plain `String` field with no newtype or constructor validation. The note that it is "hard-coded to `local` in V1" is a comment, not an invariant.

**Blast radius:** Empty `instance_id` makes the sentinel and lock path non-interoperable. A stale lock row with `instance_id = ""` is never reclaimed by PID-based cleanup keyed on `holder_pid`. The ownership sentinel fails to protect against the two-Trilithon-one-Caddy failure mode.

**Recommended mitigation:** Introduce `InstanceId(String)` newtype with a `try_new` constructor that rejects empty strings and strings containing whitespace or control characters. Use it as the `instance_id` field type on `CaddyApplier`.

---

### F030 — `ReloadKind::Abrupt` is silently dead; future abrupt reloads will be misattributed as graceful in the audit log
**Severity:** LOW
**Category:** logic-flaw
**Slice:** 7.2 / 7.7

**Attack:** The algorithm always produces `ReloadKind::Graceful`. No code path sets `Abrupt`. If Phase 12 introduces an abrupt reload (e.g., `DELETE /config` + `POST /load` to recover from a stuck graceful drain) and the implementer copy-pastes `reload_kind: ReloadKind::Graceful` from the happy path, all compile-time exhaustiveness checks pass. The audit log silently records `Graceful` for a disruptive reload, misleading operators who review connection-drop incidents.

**Why the design doesn't handle it:** Rust exhaustiveness enforcement catches unmatched arms on the read side but does not enforce that the write path (constructing `ApplyAuditNotes`) explicitly considers all variants.

**Blast radius:** Low at runtime. High for forensic accuracy: operators reviewing why connections were dropped during a rollback will see "graceful reload" and incorrectly conclude no connections were affected.

**Recommended mitigation:** Add a code comment on `ReloadKind::Abrupt` marking it "reserved for Phase 12 emergency path; any new apply path MUST consciously choose `Graceful` or `Abrupt`." Add a test asserting that at least one test produces a `ReloadKind::Abrupt` audit row (will fail until Phase 12 wires it, serving as a reminder). Alternatively, use a `#[must_use]` builder that requires the caller to explicitly choose.

---

### F031 — `snapshot_id` hashes Trilithon canonical JSON; the bytes sent to Caddy are structurally different — operators cannot verify running config against a `snapshot_id`
**Severity:** MEDIUM
**Category:** logic-flaw
**Slice:** 7.1 / 7.5

**Attack:** `snapshot_id = SHA-256(desired_state_json)` where `desired_state_json` is the Trilithon canonical JSON of `DesiredState`. The Caddy JSON produced by `CaddyJsonRenderer` is structurally different (Caddy server blocks, `@id`, `apps.http.servers`, etc.) — these two byte strings will never be equal. An operator queries `GET /config/` and wants to verify "does this running config correspond to `snapshot_id` X?" There is no stored hash of the Caddy JSON that was actually sent. Re-running the renderer against stored `desired_state_json` is the only verification path, but it requires the same renderer version to be available — not guaranteed across upgrades (F025).

**Why the design doesn't handle it:** The design implies `snapshot_id` identifies "the configuration that was applied," but it hashes the Trilithon representation, not what Caddy received. No `caddy_json_hash` field exists in the snapshot or audit row. Phase 7 is the first phase where both the rendering and the `snapshot_id` commitment exist simultaneously, making the gap concrete.

**Blast radius:** Forensic verification is compromised. Drift detection that uses `snapshot_id` as a cache key for "we already know Caddy is running this config" may emit false positives. Security auditors cannot independently confirm a given snapshot corresponds to a specific Caddy running state.

**Recommended mitigation:** Add `caddy_json_hash: Option<String>` (SHA-256 hex) to `Snapshot` and populate it in `CaddyApplier::apply` with the hash of the bytes actually sent to `POST /load`. `Option` preserves backward compatibility with pre-Phase-7 snapshots. Adding it now, while the schema is being extended for Phase 7, is cheaper than retrofitting later.

---

## Severity summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 (F028) |
| MEDIUM   | 3 (F027, F029, F031) |
| LOW      | 1 (F030) |

**Top concern:** F028 — if `apply_locks` does not exist when the applier first runs, both the cross-process lock and the storage error discriminant (F026) fail simultaneously, producing a silent unguarded dual-apply with a misleading audit record.

**Recommended action before Phase 7 implementation begins:** Address F028 (schema precondition in `CaddyApplier`) and F031 (add `caddy_json_hash` to `Snapshot` schema while it is being extended). F027 and F029 can be addressed in the same pass. F030 is documentation-grade and can be deferred to Phase 12.

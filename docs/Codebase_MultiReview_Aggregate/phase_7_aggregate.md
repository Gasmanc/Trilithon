# Phase 7 — Aggregate Review Plan

**Generated:** 2026-05-13T00:00:00Z  
**Reviewers:** code_adversarial, codex, gemini (timeout — no findings), glm, kimi, learnings_match, merge_review, minimax, qwen, scope_guardian, security  
**Raw findings:** ~65 across 10 active reviewers  
**Unique findings:** 27 after clustering  
**Consensus:** 5 unanimous · 6 majority · 16 single-reviewer  
**Conflicts:** 0  
**Superseded (already fixed):** 0

---

## How to use this document

Feed this file to `/review-remediate` to drive the fix cycle. Each finding has a
unique ID (F001, F002, …) that `review-remediate` uses to track state. Do not
renumber or delete findings — append `SUPERSEDED` status instead.

---

## CRITICAL Findings

### F001 · [CRITICAL] CAS advance fires before Caddy load + equivalence — phantom applied version on failure
**Consensus:** MAJORITY · flagged by: code_adversarial, codex  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 448–507  
**Description:** `cas_advance_config_version` runs at Step 0 before any Caddy I/O. If `POST /load` fails, returns a 5xx, panics, or `verify_equivalence` disagrees, the function returns without reverting. `applied_config_version` is now permanently advanced while Caddy still runs the previous config. The panic test explicitly acknowledges this state divergence.  
**Suggestion:** Move the CAS advance to after `verify_equivalence` succeeds, or implement a `cas_rollback_config_version` called on every failure path after CAS has fired. Alternatively, wrap check + apply + advance in one transaction with rollback on failure.  
**Claude's assessment:** Agree strongly. This is a correctness invariant violation: the applied-version pointer must only advance when Caddy has demonstrably accepted and loaded the new config. The current ordering makes the DB permanently diverge from reality on any post-CAS failure.

---

### F002 · [CRITICAL] COMMIT result silently discarded — CAS returns Ok on un-persisted transaction
**Consensus:** MAJORITY · flagged by: code_adversarial, qwen  
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 981–991  
**Description:** After `advance_config_version_if_eq` returns `Ok(new_version)`, the outer `cas_advance_config_version` issues `COMMIT` with `let _ = ...`, discarding the result. A failed COMMIT causes an implicit SQLite rollback, but the function has already returned `Ok(new_version)`. The applier proceeds as though CAS succeeded, writes audit rows, pushes config to Caddy, and returns `ApplyOutcome::Succeeded` — while `applied_config_version` in the DB is unchanged.  
**Suggestion:** Replace `let _ = sqlx::query("COMMIT")...` with `.map_err(sqlx_err)?` and propagate the COMMIT error as `StorageError`.  
**Claude's assessment:** Agree. Discarding COMMIT errors is a classic silent-data-loss bug. This must be propagated.

---

### F003 · [CRITICAL] `try_insert_lock` runs INSERT in DEFERRED transaction — TOCTOU protection broken
**Consensus:** MAJORITY · flagged by: code_adversarial [WARNING], minimax [HIGH], qwen [CRITICAL]  
**File:** `core/crates/adapters/src/storage_sqlite/locks.rs` · **Lines:** 280–299  
**Description:** `try_insert_lock` calls `pool.begin()`, which issues a DEFERRED transaction. The subsequent `BEGIN IMMEDIATE` raw query fails with "cannot start a transaction within a transaction" — an error that is silently swallowed. The INSERT therefore runs inside a DEFERRED transaction, not an IMMEDIATE one. The code comment explicitly claims TOCTOU protection via IMMEDIATE; the protection is absent.  
**Suggestion:** Use `pool.acquire().await` to get a raw connection, then execute `BEGIN IMMEDIATE` explicitly before the INSERT. Remove the `pool.begin()` call entirely. Alternatively, use `pool.begin_with(sqlx::sqlite::SqliteBegin::Immediate)` if sqlx supports it; otherwise manage the transaction manually.  
**Claude's assessment:** Agree. The learnings_match reviewer also flagged this as a known pattern (`sqlite-begin-immediate-read-check-write`). The silent swallow of the BEGIN IMMEDIATE failure means the fix appears to work in unit tests (where WAL locks don't contend) but fails under concurrent production load.

---

### F004 · [CRITICAL] Proposed Phase 7 seams not written to `seams-proposed.md` or ratified
**Consensus:** SINGLE · flagged by: merge_review (F-PMR7-001)  
**File:** `docs/architecture/seams-proposed.md` · **Lines:** general  
**Description:** The Phase 7 tagging audit identified five architectural seams introduced by cross-cutting slices 7.4–7.7 (`applier-caddy-admin`, `applier-audit-writer`, `snapshots-config-version-cas`, `apply-lock-coordination`, `apply-audit-notes-format`). Per Foundation 2 rules, proposed seams must be written to `seams-proposed.md` before merge and ratified into `seams.md` by `/phase-merge-review`. Neither file was updated. `tests/cross_phase/` contains no test files. This would have blocked merge; in catch-up mode it is a critical super-finding.  
**Suggestion:** In the next apply-path phase (or a dedicated seam-ratification micro-phase), populate `seams-proposed.md` with the five entries, ratify them into `seams.md`, and add stub test files to `tests/cross_phase/` for each seam.  
**Claude's assessment:** Agree. The cross-phase coherence system cannot track drift for any Phase 7 symbols without seam registration. This is an infrastructure debt that compounds with each subsequent phase.

---

## HIGH Findings

### F005 · [HIGH] TLS observer spawned with empty hostnames — entire TLS issuance observation is dead code
**Consensus:** UNANIMOUS · flagged by: code_adversarial, codex, glm, kimi, minimax, qwen  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 520–526  
**Description:** `CaddyApplier::apply()` always calls `observer.observe(correlation_id, vec![], Some(sid))`. `TlsIssuanceObserver::observe` returns immediately when `hostnames.is_empty()` (tls_observer.rs line 97). No follow-up TLS audit rows (success or timeout) are ever emitted in production. The entire TLS-state separation from Slice 7.8 is inert.  
**Suggestion:** Extract managed hostnames from `desired_state.routes` (TLS-enabled virtual hosts) before the observer spawn and pass them to `observer.observe(correlation_id, hostnames, Some(sid))`. Skip spawning only when the derived set is actually empty.  
**Claude's assessment:** Agree. This is unanimous across 6 of 10 active reviewers. The feature shipped as dead code; it must be wired up to be useful.

---

### F006 · [HIGH] InMemoryStorage CAS reads MAX(snapshots.config_version) instead of applied_config_version
**Consensus:** MAJORITY · flagged by: code_adversarial, glm, minimax, scope_guardian  
**File:** `core/crates/core/src/storage/in_memory.rs` · **Lines:** 308–336  
**Description:** The `Storage` trait documents `cas_advance_config_version` as reading `applied_config_version`. The SQLite implementation reads `caddy_instances.applied_config_version`. The in-memory implementation reads `MAX(snapshots.config_version)` — the highest inserted snapshot version, which is always ≥ the applied version because snapshots are inserted before apply runs. CAS with `expected=N` may succeed in InMemoryStorage when it would fail in SQLite.  
**Suggestion:** Add an `applied_config_version: Mutex<HashMap<InstanceId, i64>>` field to `InMemoryStorage`. `current_config_version` and `cas_advance_config_version` should read/write this field instead of scanning snapshots.  
**Claude's assessment:** Agree. Tests using InMemoryStorage for CAS validation are testing different semantics than production. This divergence will mask bugs.

---

### F007 · [HIGH] `rollback()` CAS uses snapshot.config_version as expected — always conflicts
**Consensus:** MAJORITY · flagged by: code_adversarial, codex, qwen  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 569–581  
**Description:** `rollback()` retrieves the target snapshot, sets `expected = snapshot.config_version`, then calls `self.apply(&snapshot, expected)`. The CAS gate compares `expected_version` against `applied_config_version`. If the current applied version is `N` and the rollback target is version `M < N`, the CAS will conflict unless `M == N`. Rollback to any version older than the immediately prior one is structurally non-functional.  
**Suggestion:** `rollback()` should read the current `applied_config_version` from storage and pass it as `expected`, or use a dedicated `force_apply` path that writes `applied_config_version = target_snapshot.config_version` directly without going through the CAS gate.  
**Claude's assessment:** Agree. Rollback is a core operation and must work for non-sequential reverts.

---

### F008 · [HIGH] Duplicate `sort_keys` + `notes_to_string` across two adapter files
**Consensus:** UNANIMOUS · flagged by: glm, kimi, merge_review (F-PMR7-002, F-PMR7-003), qwen, scope_guardian  
**File:** `core/crates/adapters/src/applier_caddy.rs` (lines 93–114) and `core/crates/adapters/src/tls_observer.rs` (lines 61–83)  
**Description:** Structurally identical `notes_to_string` and `sort_keys` helpers appear in both modules. The merge review notes that `render::canonical_json_bytes` is already `pub` and could replace both. Two separate implementations of the same sort guarantee means any format change requires parallel updates in two files.  
**Suggestion:** Replace both local copies with a call to `trilithon_core::reconciler::render::canonical_json_bytes` (already `pub`), or expose a `canonical_value_to_string` helper from `core::canonical_json` and use it from both adapter files. Delete all local copies.  
**Claude's assessment:** Agree. This hits the project's "three uses before extracting" rule from the other direction — two copies of identical code with no extraction is a violation of the "reuse before new code" rule.

---

### F009 · [HIGH] `advance_config_version_if_eq` does not verify snapshot config_version matches expected+1
**Consensus:** MAJORITY · flagged by: codex, glm  
**File:** `core/crates/adapters/src/storage_sqlite/snapshots.rs` · **Lines:** 81–108  
**Description:** The CAS check verifies snapshot existence (`SELECT COUNT(*) FROM snapshots WHERE id = ? AND caddy_instance_id = ?`) but does not verify that the snapshot's `config_version` equals `expected_version + 1`. A snapshot with a mismatched version passes the existence check and advances the applied pointer to an unrelated version.  
**Suggestion:** Extend the query to also assert `config_version = expected_version + 1`, and return `StorageError::Integrity` if the count is zero.  
**Claude's assessment:** Agree. The CAS gate should be atomically verifying both that the current applied version matches `expected` AND that the new snapshot is exactly the next version.

---

### F010 · [HIGH] Stale lock reap race: `LockError::AlreadyHeld` reports own PID instead of holder's PID
**Consensus:** MAJORITY · flagged by: code_adversarial, codex  
**File:** `core/crates/adapters/src/storage_sqlite/locks.rs` · **Lines:** 214–226  
**Description:** When the first INSERT fails and the subsequent SELECT finds no row (deleted between INSERT and SELECT), the code retries. If the second INSERT also fails, it returns `LockError::AlreadyHeld { pid: holder_pid }` where `holder_pid` is the caller's own PID, not the actual holder's PID. Operators investigating lock contention will see the daemon's own PID as the contender.  
**Suggestion:** After the second failed INSERT, perform a fresh SELECT to retrieve the actual current `holder_pid` before constructing `LockError::AlreadyHeld`.  
**Claude's assessment:** Agree. This is an operational correctness issue — wrong PID in the error makes debugging lock contention impossible.

---

### F011 · [HIGH] `process_alive` shells out to PATH-looked-up `kill` — PID reuse race + security risk
**Consensus:** MAJORITY · flagged by: code_adversarial [HIGH], glm [SUGGESTION], security [WARNING]  
**File:** `core/crates/adapters/src/storage_sqlite/locks.rs` · **Lines:** 144–155  
**Description:** `process_alive` spawns `/usr/bin/kill -0 <pid>` via `std::process::Command::new("kill")` (PATH-resolved). Between stale lock detection and the shell invocation the dead process's PID can be recycled by an unrelated process, causing false-alive. Additionally, a manipulated PATH could substitute a `kill` binary that always exits 0, permanently bypassing stale lock reaping. The `nix` crate is already in `[dev-dependencies]` but not in `[dependencies]`.  
**Suggestion:** Move `nix` to `[dependencies]` with `features = ["signal", "unistd"]` and replace `Command::new("kill")` with `nix::sys::signal::kill(Pid::from_raw(pid), Signal::try_from(0).ok())` — a direct POSIX syscall.  
**Claude's assessment:** Agree on both counts: PID reuse is a real TOCTOU issue, and PATH-based resolution is a security smell in infrastructure code. The fix is straightforward.

---

### F012 · [HIGH] Advisory lock `Drop` on panic: `spawn_blocking` task completes after Mutex releases
**Consensus:** SINGLE · flagged by: code_adversarial  
**File:** `core/crates/adapters/src/storage_sqlite/locks.rs`, `core/crates/adapters/src/applier_caddy.rs` · **Lines:** locks.rs 100–125; applier_caddy.rs 432–444  
**Description:** On panic inside the async apply block, `advisory_lock` drops first: its `Drop` impl calls `tokio::task::spawn_blocking(...)`, which submits a background task and returns immediately without awaiting. Then `_process_guard` drops, releasing the in-process `Mutex`. A subsequent caller can acquire the Mutex and proceed to `acquire_apply_lock` before the background `DELETE FROM apply_locks` executes.  
**Suggestion:** Consider holding the Mutex guard inside the async block rather than outside it, and only releasing it after `advisory_lock.release().await` completes.  
**Claude's assessment:** Agree. This is a cascade construction: correct individually, but the ordering of drops creates a window. The fix may require restructuring the lock lifecycle.

---

### F013 · [HIGH] 5xx Caddy response silently mapped to `ApplyError::Storage` — wrong taxonomy, no audit row
**Consensus:** MAJORITY · flagged by: code_adversarial [WARNING], codex [HIGH], kimi [WARNING]  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 258–302  
**Description:** In `load_or_fail`, the catch-all `Err(other_err)` arm maps 5xx `BadStatus` responses (and other non-4xx errors) to `ApplyError::Storage`. No `config.apply-failed` audit row is written. `ApplyFailureKind::CaddyServerError` exists but is never constructed.  
**Suggestion:** Add an explicit `5xx BadStatus` match arm that: (1) writes a `config.apply-failed` audit row with `error_kind = "CaddyServerError"`, (2) returns `Ok(ApplyOutcome::Failed { kind: ApplyFailureKind::CaddyServerError, .. })`.  
**Claude's assessment:** Agree. Misclassifying 5xx as a storage error hides Caddy-side failures from the audit trail and breaks the failure taxonomy.

---

## WARNING Findings

### F014 · [WARNING] Post-load equivalence check maps all `CaddyError` variants to `ApplyError::Unreachable`
**Consensus:** SINGLE · flagged by: kimi  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 307–313  
**Description:** `verify_equivalence` maps any `CaddyError` from `get_running_config` to `ApplyError::Unreachable`, including 5xx `BadStatus` and `ProtocolViolation`. A reachable-but-failing Caddy is misreported as down.  
**Suggestion:** Distinguish `CaddyError::Unreachable`/`Timeout` (→ `ApplyError::Unreachable`) from `BadStatus(5xx)` and `ProtocolViolation` (→ `ApplyError::CaddyRejected` or a new variant).  
**Claude's assessment:** Agree. This is a caller-visible semantic error that misleads retry logic.

---

### F015 · [WARNING] `advance_config_version_if_eq` UPDATE doesn't check `rows_affected` — silent no-op on missing instance row
**Consensus:** SINGLE · flagged by: kimi  
**File:** `core/crates/adapters/src/storage_sqlite/snapshots.rs` · **Lines:** 100–106  
**Description:** The UPDATE that advances `applied_config_version` may affect zero rows if the `caddy_instances` row is missing or was deleted. The function returns `Ok(new_version)` regardless — the caller believes CAS succeeded when nothing was persisted.  
**Suggestion:** Assert `query_result.rows_affected() == 1` after the UPDATE; return `Err(StorageError::Integrity { detail: "instance row missing" })` otherwise.  
**Claude's assessment:** Agree. This is a defensive correctness fix that prevents silent phantom-success when the instance has been deleted.

---

### F016 · [WARNING] Invalid preset JSON silently discarded during render
**Consensus:** MAJORITY · flagged by: kimi, security  
**File:** `core/crates/core/src/reconciler/render.rs` · **Lines:** 327–330  
**Description:** When embedding a policy attachment, `serde_json::from_str` failure on `preset.body_json` is silently ignored with `if let Ok(body) = ...`. A corrupted or non-JSON preset body disappears from the rendered config without raising an error, producing a successful-looking render with a missing policy.  
**Suggestion:** Treat parse failure as a render error (add `RenderError::InvalidPresetBody`) rather than silently omitting the policy.  
**Claude's assessment:** Agree. Silent omission of a security policy body from a rendered config is a correctness and security concern.

---

### F017 · [WARNING] Conflict audit note uses hand-rolled format! string instead of `notes_to_string`
**Consensus:** SINGLE · flagged by: code_adversarial  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 359  
**Description:** `handle_conflict` builds its audit note with a `format!()` string literal rather than constructing an `ApplyAuditNotes` struct and calling `notes_to_string`. This bypasses the key-sorting path and diverges from every other audit row written by the applier.  
**Suggestion:** Add `stale_version` and `current_version` fields to `ApplyAuditNotes` and route `handle_conflict` through `notes_to_string`. (After F008 is fixed, this should call the shared canonical helper.)  
**Claude's assessment:** Agree. Consistency in the audit serialisation path is important for downstream query correctness.

---

### F018 · [WARNING] `validate()` returns `Ok(ValidationReport::default())` instead of `Err(PreflightFailed)` per TODO spec
**Consensus:** SINGLE · flagged by: scope_guardian  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 564–567  
**Description:** The TODO spec states the `validate()` placeholder should return `Err(ApplyError::PreflightFailed { failures: vec![] })` to signal "not implemented, treat as failure." The implementation returns `Ok(ValidationReport::default())`, signalling "valid" to callers.  
**Suggestion:** Either add `PreflightFailed { failures: Vec<ValidationFailure> }` to `ApplyError` and return it from `validate()`, or add a code comment explaining why `Ok(ValidationReport::default())` is the correct placeholder behaviour.  
**Claude's assessment:** Agree that the discrepancy needs resolution. Returning Ok on an unimplemented preflight check means future callers may rely on validation that does nothing.

---

### F019 · [WARNING] IPv6 upstream addresses rendered without brackets — ambiguous dial string
**Consensus:** SINGLE · flagged by: kimi  
**File:** `core/crates/core/src/reconciler/render.rs` · **Lines:** 353–356  
**Description:** `resolve_upstream_dial` formats TCP upstreams as `{host}:{port}`. For IPv6 addresses (e.g. `::1`) this produces `::1:8080` instead of the bracketed form `[::1]:8080` that Caddy's `dial` field requires.  
**Suggestion:** Detect IPv6 addresses (colon-containing hosts) and wrap them in brackets: `format!("[{host}]:{port}")` when `host.contains(':')`.  
**Claude's assessment:** Agree. This is a correctness bug for IPv6 upstreams.

---

### F020 · [WARNING] UNIX socket path and Docker container ID passed unvalidated to Caddy config
**Consensus:** SINGLE · flagged by: security  
**File:** `core/crates/core/src/reconciler/render.rs` · **Lines:** 357–359  
**Description:** `UpstreamDestination::UnixSocket { path }` and `DockerContainer { container_id, port }` are inserted into rendered Caddy JSON without sanitisation. A path containing `../../` segments or a container_id with shell metacharacters passes through directly to Caddy's transport layer.  
**Suggestion:** Validate `UnixSocket` paths reject `..`, null bytes, and non-absolute paths. Validate `container_id` matches `[a-zA-Z0-9_.\-]{1,128}`.  
**Claude's assessment:** Agree. Input validation at the rendering boundary is a standard security control, especially for values that reach an external process (Caddy).

---

### F021 · [WARNING] `correlation_id` silently replaced with fresh ULID on parse failure — breaks audit trail
**Consensus:** SINGLE · flagged by: security  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 424–427  
**Description:** `snapshot.correlation_id.parse::<Ulid>().unwrap_or_else(|_| Ulid::new())` silently generates a new correlation ID when the stored value cannot be parsed. Audit rows will carry a different ID than mutation-pipeline rows, breaking trace correlation without any warning.  
**Suggestion:** Log a `tracing::warn!` when the fallback fires, including the raw `snapshot.correlation_id` value and `snapshot_id`.  
**Claude's assessment:** Agree. Silent ID replacement in an audit trail is a correctness gap; at minimum a warn log is required.

---

### F022 · [WARNING] `ApplyAuditNotes` doc comment references `to_canonical_bytes` — wrong serialiser
**Consensus:** SINGLE · flagged by: merge_review (F-PMR7-004)  
**File:** `core/crates/core/src/reconciler/applier.rs` · **Lines:** 251  
**Description:** The doc comment states serialisation uses `trilithon_core::canonical_json::to_canonical_bytes`, but that function only accepts `&DesiredState`, not `&ApplyAuditNotes`. Actual serialisation is via the local `notes_to_string` helpers. Future phase authors reading this will wrongly assume the bytes are canonicalised to the same spec as `DesiredState`.  
**Suggestion:** Correct the doc comment to describe the actual serialisation path. Once F008 is applied (shared canonical helper), update accordingly.  
**Claude's assessment:** Agree. Misleading doc comments on contract types are a cross-phase correctness risk.

---

## SUGGESTION / LOW Findings

### F023 · [SUGGESTION] `AcquiredLock::drop` spawns new Tokio runtime — heavyweight and structurally unnecessary
**Consensus:** SINGLE · flagged by: qwen  
**File:** `core/crates/adapters/src/storage_sqlite/locks.rs` · **Lines:** 96–131  
**Description:** `AcquiredLock::drop` constructs a fresh `CurrentThread` Tokio runtime to run a single DELETE query. This is heavyweight and `mem::forget` in `release()` makes the path unreachable in the happy case — it only fires on a panic/forgotten drop.  
**Suggestion:** Use `tokio::task::block_in_place` instead of spawning a blocking task with a nested runtime.  
**Claude's assessment:** Agree — a nested runtime in Drop is unusual. However, F012 (the panic-drop ordering issue) should be addressed first; the fix to F012 may change the drop structure entirely.

---

### F024 · [SUGGESTION] `bounded_excerpt` can produce output 3 bytes over the stated maximum
**Consensus:** SINGLE · flagged by: qwen  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 73–84  
**Description:** When input exceeds `EXCERPT_MAX_BYTES` (512), the function truncates then appends the UTF-8 ellipsis (3 bytes). The result can be 515 bytes — exceeding the stated maximum.  
**Suggestion:** Reserve 3 bytes for the ellipsis by truncating at `EXCERPT_MAX_BYTES - 3`.  
**Claude's assessment:** Agree — a minor off-by-three correctness issue.

---

### F025 · [SUGGESTION] Conflict outcome versions swapped in `handle_conflict` call arguments
**Consensus:** SINGLE · flagged by: qwen  
**File:** `core/crates/adapters/src/applier_caddy.rs` · **Lines:** 458–461  
**Description:** `handle_conflict` is called with `(expected, observed)` but the parameters are named `(stale_version, current_version)`. The destructuring may swap `expected` and `observed`, writing incorrect version numbers into `ApplyOutcome::Conflicted` and `mutation.conflicted` audit rows.  
**Suggestion:** Audit the call site: verify whether `expected` maps to `stale_version` and `observed` to `current_version`. If swapped, fix the call or rename the parameters to match.  
**Claude's assessment:** Plausible — worth verifying at the code level. If the swap exists, audit rows and retry logic will be systematically incorrect.

---

### F026 · [SUGGESTION] Preset body JSON embedded in Caddy handler without structural validation
**Consensus:** MAJORITY · flagged by: security (suggestion), kimi (warning — already F016 covers silent discard)  
**File:** `core/crates/core/src/reconciler/render.rs` · **Lines:** 327–330  
**Description:** After parsing succeeds, the preset body `Value` is embedded directly as `"policy"` in the Caddy handler object without structural validation against an allowlist of permitted keys.  
**Suggestion:** Either validate preset body JSON against a restricted key allowlist before storing in `state.presets`, or enumerate the body's top-level keys during the capability check.  
**Claude's assessment:** Agree in principle, though this is partially mitigated by Caddy itself rejecting unknown handler keys. An allowlist adds defence-in-depth.

---

### F027 · [SUGGESTION] `contract-roots.toml` not updated for Phase 7 public API surface
**Consensus:** SINGLE · flagged by: merge_review (F-PMR7-005)  
**File:** `docs/architecture/contract-roots.toml` · **Lines:** general  
**Description:** Phase 7 introduced ~14 new `pub` symbols across `core` and `adapters` (Applier trait, ApplyOutcome, ApplyAuditNotes, CaddyApplier, LockError, TlsIssuanceObserver, etc.). None appear in `contract-roots.toml`. Future phases (Phase 9 HTTP layer, Phase 13 query path) will structurally depend on these symbols without drift tracking.  
**Suggestion:** Populate `contract-roots.toml` with at minimum the `core` symbols: `Applier` trait, `ApplyOutcome`, `ApplyAuditNotes`, `AppliedState`, `ReloadKind`, `ApplyError`. Run `cargo xtask registry-extract` when available to regenerate `contracts.md`.  
**Claude's assessment:** Agree. Low priority because the registry is uniformly empty, but Phase 7 is the first phase introducing cross-cutting contracts — leaving them unregistered compounds the coherence debt.

---

## CONFLICTS (require human decision before fixing)

*No conflicts identified.*

---

## Out-of-scope / Superseded

| ID | Title | Reason |
|----|-------|--------|
| — | Learnings match pattern warnings (5 entries) | General advisory reminders (BEGIN IMMEDIATE, rollback early-exit, version overflow, audit-diff, apply-layer self-defend) — not independent findings; root causes covered by F001–F003. |
| — | Gemini reviewer | Timed out — no findings produced. |
| — | Migration filename mismatch in TODO spec | Scope_guardian flagged TODO spec text says `0004_apply_locks.sql` but file is `0007_apply_locks.sql`. No code change needed — doc-only fix in the TODO file, out of aggregate scope. |
| — | NoOpDiffEngine in core | Scope_guardian suggestion to move to `#[cfg(test)]`. Deferred: this is a design preference, not a correctness or safety issue. Low priority. |

---

## Summary statistics

| Severity | Unanimous | Majority | Single | Total |
|----------|-----------|----------|--------|-------|
| CRITICAL | 0 | 3 | 1 | 4 |
| HIGH | 2 | 5 | 2 | 9 |
| WARNING | 0 | 1 | 8 | 9 |
| SUGGESTION | 0 | 1 | 4 | 5 |
| **Total** | **2** | **10** | **15** | **27** |

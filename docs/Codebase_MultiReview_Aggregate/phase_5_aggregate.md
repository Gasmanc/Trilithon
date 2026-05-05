# Phase 5 — Aggregate Review Plan

**Generated:** 2026-05-05T00:00:00Z
**Reviewers:** code_adversarial, codex, gemini, glm, kimi, minimax, qwen, scope_guardian, security
**Raw findings:** 48 across 9 reviewers
**Unique findings:** 25 after clustering
**Consensus:** 0 unanimous · 1 majority (6/9) · 8 multi-reviewer (2–4) · 16 single-reviewer
**Conflicts:** 0
**Superseded (already fixed):** 1

---

## How to use this document

Feed this file to `/review-remediate` to drive the fix cycle. Each finding has a
unique ID (F001, F002, …) that `review-remediate` uses to track state. Do not
renumber or delete findings — append `SUPERSEDED` status instead.

---

## CRITICAL Findings

### F001 · [CRITICAL] Canonicalizer corrupts large integers via f64 round-trip
**Consensus:** MULTI · flagged by: codex (CRITICAL), gemini (CRITICAL), kimi (WARNING)
**File:** `core/crates/core/src/canonical_json.rs` · **Lines:** 65–88
**Description:** `canonicalise_value` routes all `Value::Number` variants through `as_f64()` before deciding whether to store as integer. IEEE-754 f64 can only represent integers exactly up to ±2^53. Any i64/u64 value above that threshold is silently rounded, producing a different bit pattern — and therefore a different content address — for the same logical state. This corrupts deduplication and content-addressed storage.
**Suggestion:** Check `n.is_i64()` and `n.is_u64()` first and leave them untouched. Only call `as_f64()` for values already known to be floating-point. Remove the `f as i64` cast-back path entirely.
**Claude's assessment:** Agree — confirmed by two CRITICAL flags and one WARNING from independent reviewers. The f64 round-trip is objectively lossy above 2^53 and the only safe fix is to avoid the cast. High priority; content addressing is a core invariant.

---

### F002 · [CRITICAL] Monotonicity check raceable under concurrent writers
**Consensus:** SINGLE · flagged by: codex (CRITICAL); gemini (SUGGESTION, same root)
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 349–401
**Description:** `insert_snapshot` begins a DEFERRED transaction, reads `MAX(config_version)`, then inserts. Two concurrent writers can observe the same max and both pass the monotonicity guard, allowing a row with a lower `config_version` to be committed after a row with a higher one — violating strict monotonicity.
**Suggestion:** Use `BEGIN IMMEDIATE` (or sqlx's `begin_with(ImmediateTransaction)`) so the write lock is held from the start of the read-check-insert sequence. Alternatively enforce monotonicity with a DB trigger or CHECK constraint.
**Claude's assessment:** Agree — textbook TOCTOU race on a read-check-write. The concurrency path may be rare in V1 single-daemon deployments, but DEFERRED transactions are an incorrect choice here regardless. Codex flags CRITICAL; gemini independently identifies the same root cause.

---

## HIGH Findings

### F003 · [HIGH] Hardcoded `caddy_instance_id = 'local'` breaks multi-instance monotonicity
**Consensus:** MAJORITY · flagged by: code_adversarial (HIGH), minimax (HIGH), qwen (WARNING), kimi (WARNING), security (WARNING), glm (SUGGESTION) — 6/9 reviewers
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 389, 411
**Description:** Both the `MAX(config_version)` monotonicity guard and the INSERT bind `caddy_instance_id = 'local'` as a string literal. The `Snapshot` struct carries no `caddy_instance_id` field. A second Caddy instance would always check against the 'local' partition, allowing config_version = 1 to be re-used silently. All fetch queries (`fetch_by_config_version`, `fetch_by_date_range`, etc.) similarly omit instance filtering.
**Suggestion:** Add `caddy_instance_id: String` to `Snapshot`. Bind it in the INSERT and the monotonicity MAX query. Add WHERE clauses to all fetch methods. At minimum, document the V1 single-instance constraint with a tracked suppression comment if multi-instance is explicitly deferred.
**Claude's assessment:** Agree — 6 of 9 reviewers flag this. The hardcoding is fine for V1 single-instance deployment, but it must either be removed or clearly documented with a tracked `zd:` comment so future multi-instance work has an obvious hook. The silent monotonicity bypass for other instances is a real correctness risk.

---

### F004 · [HIGH] `InMemoryStorage` diverges from `SqliteStorage` on duplicate semantics
**Consensus:** MULTI · flagged by: code_adversarial (HIGH), glm (WARNING), qwen (SUGGESTION) — 3/9 reviewers
**File:** `core/crates/core/src/storage/in_memory.rs` · **Lines:** 67–93
**Description:** `InMemoryStorage::insert_snapshot` returns `Err(StorageError::SnapshotDuplicate)` whenever the id already exists. `SqliteStorage::insert_snapshot` returns `Ok(existing_id)` when the body is byte-equal (idempotent dedup) and only errors on hash collision (body differs). Any caller that passes integration tests using `InMemoryStorage` and is then deployed against `SqliteStorage` will observe different behaviour on the retry path.
**Suggestion:** Add a body-equality check in `InMemoryStorage`: if id exists and body is equal, return `Ok(id)`; if id exists and body differs, return `Err(StorageError::SnapshotHashCollision)`. This aligns the two implementations.
**Claude's assessment:** Agree — a test double that behaves differently from the real implementation on an idempotent path undermines the test suite. The fix is straightforward and low-risk.

---

### F005 · [HIGH] Deduplication path returns early inside an open transaction
**Consensus:** SINGLE · flagged by: code_adversarial (HIGH)
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 360–365
**Description:** When a byte-equal duplicate is detected, `return Ok(SnapshotId(id))` fires inside an open `let mut tx = self.pool.begin()...` scope. The transaction is dropped without explicit commit or rollback. sqlx's Drop impl does roll back, but this pattern is one future edit away from a resource leak or a missed error on the rollback itself.
**Suggestion:** Call `tx.rollback().await?` (or `drop(tx)` with a comment) explicitly before each early-return path.
**Claude's assessment:** Agree — the current code is technically safe due to sqlx's Drop behaviour, but the implicit rollback is a code smell. The fix is a one-liner and removes a latent danger.

---

### F006 · [HIGH] `canonical_json_version` not persisted to the database
**Consensus:** MULTI · flagged by: gemini (HIGH), kimi (HIGH), qwen (SUGGESTION) — 3/9 reviewers
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 296–330, 404–428
**Description:** `Snapshot.canonical_json_version` exists in the Rust struct and README documents it as stored per-row for future format migration detection, but no DB column exists. `row_to_snapshot` always overwrites the field with the current constant. When the constant is incremented in a future format change, all historical rows will be reported as using the new format, breaking migration detection.
**Suggestion:** Add a migration (e.g. `0005_canonical_json_version.sql`) with `ALTER TABLE snapshots ADD COLUMN canonical_json_version INTEGER NOT NULL DEFAULT 1`. Bind it on INSERT; read it back in `row_to_snapshot`.
**Claude's assessment:** Agree — the README explicitly documents this as a stored field. The omission is a pre-existing design gap that will only become painful when a format bump happens. Adding the column now is low-effort.

---

### F007 · [HIGH] Snapshot content hash not verified in write path — `snapshot_id` accepted verbatim
**Consensus:** MULTI · flagged by: scope_guardian (HIGH), kimi (HIGH) — 2/9 reviewers
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 336–430
**Description:** `insert_snapshot` uses `snapshot.snapshot_id.0` as the primary key without verifying it is actually the SHA-256 of `snapshot.desired_state_json`. A caller can persist an arbitrary id for any body, silently breaking content-addressing. The acceptance criterion in the TODO states the writer "MUST compute the canonical hash."
**Suggestion:** At the top of `insert_snapshot`, recompute `canonical_json::content_address(&snapshot.desired_state_json)` (or equivalent bytes variant) and return `StorageError::Integrity` if it does not match `snapshot.snapshot_id.0`.
**Claude's assessment:** Agree — content-addressed storage that trusts the caller to supply the correct address is not content-addressed storage. The verification is the single most important invariant and it is missing. High priority.

---

## WARNING Findings

### F008 · [WARNING] `created_at_monotonic_nanos` is a wall-clock value, not a monotonic counter
**Consensus:** MULTI · flagged by: gemini (WARNING), kimi (WARNING), qwen (WARNING), glm (SUGGESTION) — 4/9 reviewers
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 296–330, 338–433
**Description:** `row_to_snapshot` populates `created_at_monotonic_nanos` by multiplying `created_at_ms` (wall-clock epoch ms) by 1,000,000. The field name and documentation imply a monotonic clock source; the value is actually epoch nanoseconds derived from a wall clock, which can regress on NTP corrections. Additionally, the precision above milliseconds is fabricated (all sub-ms bits are zero).
**Suggestion:** Either: (a) rename the Rust field to `created_at_epoch_nanos` and update docs to drop the "monotonic" claim; or (b) add a dedicated `monotonic_nanos` INTEGER column populated from a true monotonic clock. Option (a) is lower-risk for V1.
**Claude's assessment:** Agree — the naming is actively misleading and documents a guarantee that cannot be met. At minimum rename the field. A separate monotonic column can be deferred.

---

### F009 · [WARNING] Dedup early-return bypasses config_version monotonicity check
**Consensus:** SINGLE · flagged by: code_adversarial (WARNING)
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 353–402
**Description:** The deduplication check (byte-equal body → return existing id) fires before the monotonicity guard. A caller can re-submit a deduplicated snapshot whose config_version is lower than the current max and receive a silent `Ok` — even when that config_version should fail the monotonicity check on a fresh insert.
**Suggestion:** On the dedup-exit path, verify that the incoming `config_version` matches the stored row's `config_version`. Return `StorageError::SnapshotVersionMismatch` if they differ.
**Claude's assessment:** Agree — the ordering creates an exploitable bypass. Whether this matters in practice depends on how callers use the dedup path, but the fix is a single equality check before the early return.

---

### F010 · [WARNING] `fetch_by_date_range` with empty range performs unbounded full table scan
**Consensus:** SINGLE · flagged by: code_adversarial (WARNING)
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 166–205
**Description:** When `SnapshotDateRange { since: None, until: None }` is passed, the query omits the WHERE clause entirely and returns all rows with no LIMIT. The snapshots table grows unboundedly over daemon lifetime; this call can return hundreds of thousands of rows.
**Suggestion:** Add a mandatory `limit: u32` parameter (with a documented maximum, e.g. 1000) or enforce a hardcoded cap in the query when both bounds are absent.
**Claude's assessment:** Agree — unbounded queries on an append-only table are an operational hazard. A limit is straightforward to add.

---

### F011 · [WARNING] `fetch_by_date_range` builds SQL with `format!` — fragile pattern
**Consensus:** MULTI · flagged by: security (WARNING), minimax (WARNING) — 2/9 reviewers
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 178–205
**Description:** The WHERE clause is assembled with `format!(r"... {where_clause} ...")` interpolating a runtime-constructed string directly into the SQL. Currently the content is from static literals only, so no injection today. However, the pattern means any future addition of a user-controlled sort column or filter will be interpolated without binding, creating a SQL injection vector.
**Suggestion:** Replace with four fully static query strings (one per combination of since/until present/absent) selected by an `if`/`match`. This eliminates the pattern entirely.
**Claude's assessment:** Agree — the current code is safe but the pattern is a trap for future contributors. The fix is mechanical and reduces cognitive load on reviewers.

---

### F012 · [WARNING] Immutability triggers absent until migration 0004 runs
**Consensus:** SINGLE · flagged by: code_adversarial (WARNING)
**File:** `core/crates/adapters/migrations/0004_snapshots_immutable.sql` · **Lines:** general
**Description:** The `snapshots_no_update` and `snapshots_no_delete` triggers are installed by migration 0004. Any code path that inserts snapshots between `open()` returning and `apply_migrations()` completing operates on a database without the immutability guarantee. If migrations fail partway, the triggers may never be installed.
**Suggestion:** Make `SqliteStorage::open` unconditionally apply migrations before returning, or add a startup assertion that the applied migration version is ≥ 4.
**Claude's assessment:** Agree in principle, though the window is narrow. Confirming migrations apply before returning from `open()` is the right invariant to enforce regardless.

---

### F013 · [WARNING] `sort_unstable_by` on JSON object keys does not guarantee stable order for duplicate keys
**Consensus:** SINGLE · flagged by: code_adversarial (WARNING)
**File:** `core/crates/core/src/canonical_json.rs` · **Lines:** 69–71
**Description:** `canonicalise_value` flattens object entries and sorts with `sort_unstable_by`. If a custom `Serialize` impl emits duplicate keys (which serde_json allows in principle), two logically identical maps could produce different canonical bytes and thus different content addresses.
**Suggestion:** Add duplicate-key detection in `canonicalise_value` and return an error if duplicates are found; or switch to `sort_by` (stable) and document that duplicate keys produce undefined behaviour.
**Claude's assessment:** Partially agree — duplicate keys in serde_json are extremely rare in practice, but for a canonical-JSON implementation the correctness case is absolute. Adding a duplicate-key check is the safer path.

---

### F014 · [WARNING] `InMemoryStorage` ABBA lock ordering deadlock risk
**Consensus:** SINGLE · flagged by: glm (WARNING)
**File:** `core/crates/core/src/storage/in_memory.rs` · **Lines:** 67–69, 139–141
**Description:** `insert_snapshot` acquires locks in the order `snapshots → latest_ptr`, but `latest_desired_state` acquires them in `latest_ptr → snapshots`. Two tokio tasks interleaving these methods can produce an ABBA deadlock.
**Suggestion:** Establish a canonical lock acquisition order (e.g. always `snapshots` first, then `latest_ptr`) and apply it consistently. Add a comment at each acquisition site documenting the ordering.
**Claude's assessment:** Agree — ABBA lock ordering is a classic deadlock pattern. The fix is low-effort: pick one order, document it, apply it everywhere.

---

### F015 · [WARNING] `Snapshot::intent` doc promises a constructor that doesn't exist; enforcement missing
**Consensus:** MULTI · flagged by: glm (WARNING), kimi (SUGGESTION), security (WARNING) — 3/9 reviewers
**File:** `core/crates/core/src/storage/types.rs` · **Lines:** 62–98
**Description:** The doc comment on `Snapshot::intent` states "the field is intentionally private to the serialiser" and references a `Snapshot::new` constructor, but `intent` is declared `pub` and no `Snapshot::new` exists. `validate_intent` is `#[must_use]` but is never called in the production write path (`insert_snapshot`), meaning an oversized intent (up to SQLite's 1 GB text limit) can be stored without bound-checking.
**Suggestion:** Either: (a) make `intent` private and add a fallible `Snapshot::new` constructor that calls `validate_intent`; or (b) add an explicit check at the top of `insert_snapshot` returning `StorageError` when `!Snapshot::validate_intent(&snapshot.intent)`. Also update the doc comment to match reality.
**Claude's assessment:** Agree — the stale doc comment is actively misleading and the missing enforcement is a real input validation gap. Option (a) is cleaner but (b) is a one-liner fix.

---

### F016 · [WARNING] Lint suppressions missing required `zd:` tracked-id format
**Consensus:** MULTI · flagged by: glm (WARNING), minimax (SUGGESTION) — 2/9 reviewers
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 307, 345
**Description:** `#[allow(clippy::cast_sign_loss)]` and `#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]` have `// reason:` comments but do not include the project-mandated `zd:<id> expires:<YYYY-MM-DD> reason:<short>` format. The project constitution requires a tracked id for any suppression.
**Suggestion:** Update inline comments to: `// zd:P5-001 expires:2027-01-01 reason: cast is safe because created_at_ms is always a non-negative epoch value`.
**Claude's assessment:** Agree — this is a direct violation of the project constitution. Trivial to fix.

---

### F017 · [WARNING] `fetch_by_parent_id` sort order inconsistent with other fetch methods
**Consensus:** SINGLE · flagged by: minimax (WARNING)
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 182–195
**Description:** `fetch_by_parent_id` hardcodes `ORDER BY config_version ASC` while `fetch_by_config_version` uses `ORDER BY created_at ASC`. The inconsistency is undocumented and may produce surprising results when config_version is not strictly sequential.
**Suggestion:** Document why the sort order differs, or align to a single canonical ordering (e.g. `ORDER BY created_at ASC` everywhere with `config_version` as a tiebreaker).
**Claude's assessment:** Weakly agree — the inconsistency is real but may be intentional (lineage queries naturally order by version). Needs a comment at minimum.

---

### F018 · [WARNING] Broken ADR link in `core/README.md`
**Consensus:** SINGLE · flagged by: codex (WARNING)
**File:** `core/README.md` · **Lines:** 111
**Description:** The link target `docs/adr/0009-...` is relative to `core/README.md`, so it resolves to `core/docs/adr/...` (nonexistent) instead of the repository-level `docs/adr/...`.
**Suggestion:** Update the link to `../docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md`.
**Claude's assessment:** Agree — trivial fix, confirmed the relative path is wrong.

---

### F019 · [WARNING] `SnapshotWriter` is not a named struct — rolled into `SqliteStorage`
**Consensus:** SINGLE · flagged by: scope_guardian (WARNING)
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** general
**Description:** The Phase 5 TODO specifies "Implement the `SnapshotWriter` adapter" as a named type. The diff implements all required logic as methods on `SqliteStorage` directly. No named `SnapshotWriter` type exists post-diff.
**Suggestion:** Either introduce a `SnapshotWriter` newtype wrapping `SqliteStorage` (or a separate impl block behind a trait), or add a comment in the phase doc and `SqliteStorage` stating that `SnapshotWriter` is the insert_snapshot impl on `SqliteStorage`.
**Claude's assessment:** Weakly agree — if the phase TODO was aspirational about naming but the impl is functionally complete, adding a clarifying comment is sufficient. A newtype has merit only if `SnapshotWriter` is intended to be a distinct public API surface.

---

### F020 · [WARNING] Monotonicity property test uses deterministic loop, not proptest
**Consensus:** SINGLE · flagged by: scope_guardian (WARNING)
**File:** `core/crates/adapters/tests/snapshot.rs` · **Lines:** 608–705
**Description:** The TODO states "A property test MUST assert strict monotonic increase per instance across interleaved writes." The test is a sequential deterministic loop (module named `props` but no proptest crate used). It does not exercise random interleaving.
**Suggestion:** Either add `proptest` and convert the test to a `proptest!` macro with randomised ordering, or explicitly document (in the test module and phase doc) that the deterministic exhaustive loop was accepted as the substitute.
**Claude's assessment:** Agree — the `props` module name implies proptest was intended. The TODO says "MUST". Either convert or document the accepted deviation.

---

## SUGGESTION / LOW Findings

### F021 · [SUGGESTION] Two `content_address` functions perform identical SHA-256 hashing
**Consensus:** SINGLE · flagged by: glm
**File:** `core/crates/core/src/canonical_json.rs` · **Lines:** general
**Description:** Two `content_address` functions exist with different signatures (`&DesiredState` vs `&[u8]`), both performing SHA-256. If the algorithm changes, both must be updated independently.
**Suggestion:** Unify to a single canonical entry point that accepts bytes; the `&DesiredState` variant becomes a thin wrapper.
**Claude's assessment:** Agree — the duplication is small but real. Converging on one function is the right move before any algorithm change.

---

### F022 · [SUGGESTION] `SnapshotId` accepts arbitrary strings without hex validation
**Consensus:** MULTI · flagged by: kimi (SUGGESTION), security (SUGGESTION) — 2/9 reviewers
**File:** `core/crates/core/src/storage/types.rs` · **Lines:** 31–32
**Description:** `SnapshotId` is an unvalidated `String` wrapper. The 64-character lowercase-hex invariant is documented but not enforced. A caller supplying an arbitrary string as the ID could reach misleading error paths (e.g. `SnapshotHashCollision`).
**Suggestion:** Add a `TryFrom<String>` (or fallible `new`) that validates the input is exactly 64 ASCII lowercase hex digits. Use `SnapshotId::try_from` at the storage boundary.
**Claude's assessment:** Agree — enforcing the invariant at construction time is cleaner than validating on every use. Pairs well with F007 (content hash verification).

---

### F023 · [SUGGESTION] `let _ = parse_actor_kind(...)` pattern is unclear
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 305–306
**Description:** `let _ = parse_actor_kind(&actor_kind_str)?` parses for validation only but `let _` makes the intent unclear — it looks like a forgotten binding.
**Suggestion:** Use `parse_actor_kind(&actor_kind_str)?;` (statement form, no binding) to make it clear the parsed value is intentionally discarded.
**Claude's assessment:** Agree — trivial cleanup; the statement form is idiomatic Rust for parse-for-validation.

---

### F024 · [SUGGESTION] `actor_kind` silently discarded on read; hardcoded "system" on write
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 305–306, 343
**Description:** On write, `actor_kind` is hardcoded to `"system"`. On read, it is parsed (for validation only) then discarded. The audit trail cannot distinguish user-initiated from system-initiated snapshots; existing rows with `actor_kind = 'user'` or `'token'` have that field permanently ignored.
**Suggestion:** Either preserve `actor_kind` in the `Snapshot` struct for full audit trail fidelity, or document the intentional V1 decision with a tracked issue reference.
**Claude's assessment:** Agree — the hardcoding is probably intentional for V1 but should be documented. Without documentation, a future contributor reading the audit log will be confused by the always-"system" entries.

---

### F025 · [SUGGESTION] `cast_sign_loss` suppression relies on implicit invariant
**Consensus:** SINGLE · flagged by: minimax
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 303
**Description:** `#[allow(clippy::cast_sign_loss)]` on `(created_at_ms as u64)` relies on an implicit invariant that SQLite never produces a negative timestamp. The invariant is real but undocumented.
**Suggestion:** Add a runtime assertion (`assert!(created_at_ms >= 0, "...")`) or a comment explaining the invariant. Also see F016 for the missing `zd:` format.
**Claude's assessment:** Agree — already covered in part by F016 (missing tracked id). An assertion would make the invariant explicit rather than implicit.

---

## CONFLICTS (require human decision before fixing)

_None identified. All multi-reviewer disagreements were on severity, not on fix direction._

---

## Out-of-scope / Superseded

Findings excluded from the actionable list with rationale:

| ID | Title | Reason |
|----|-------|--------|
| — | scope_guardian [HIGH]: Web frontend changes in phase range | Self-excluded by scope_guardian: "baseline formatter fixes committed before phase work — excluded from review scope." No action needed. |

---

## Summary statistics

| Severity | Unanimous | Majority (5+) | Multi (2–4) | Single | Total |
|----------|-----------|---------------|-------------|--------|-------|
| CRITICAL | 0 | 0 | 1 | 1 | 2 |
| HIGH | 0 | 1 | 2 | 2 | 5 |
| WARNING | 0 | 0 | 5 | 8 | 13 |
| SUGGESTION | 0 | 0 | 1 | 4 | 5 |
| **Total** | **0** | **1** | **9** | **15** | **25** |

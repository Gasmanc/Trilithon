# Adversarial Review — Phase 05 — Round 2

**Design summary:** Phase 5 implements a content-addressed, append-only snapshot store for Trilithon's desired-state history. The revised design adds optimistic concurrency via `expected_version: Option<i64>`, `daemon_run_id` per-process ULID tracking, a `BEGIN IMMEDIATE` transaction for atomic check-and-write, state size caps, write timeouts, and paginated fetch with a `MAX_FETCH_ROWS = 500` guard.

**Prior rounds:** Round 1 raised 11 findings (2 CRITICAL, 5 HIGH, 4 MEDIUM, 2 LOW). All 11 are addressed in the revision:
- CRITICAL-1 (missing `expected_version`): fixed — `SnapshotInputs.expected_version: Option<i64>` added with full ADR-0012 dispatch.
- CRITICAL-2 (TOCTOU on `config_version`): fixed — `BEGIN IMMEDIATE` closes the window; `VersionRace` variant added for the residual UNIQUE constraint path.
- HIGH-1 (cross-instance parent linkage): fixed — parent lookup now includes `AND caddy_instance_id = ?`.
- HIGH-2 (ambiguous unique-constraint dispatch): fixed — constraint-name parsing dispatches to `VersionRace` vs dedupe path vs `Sqlx`.
- HIGH-3 (unbounded `in_range`/`children_of`): fixed — `Page { limit, offset }` + `MAX_FETCH_ROWS = 500`.
- HIGH-4 (monotonic nanos across restarts): fixed — `daemon_run_id` column added; ordering rule documented.
- HIGH-5 (no `desired_state_json` size cap): fixed — `StateTooLarge` + `DEFAULT_MAX_DESIRED_STATE_BYTES = 10 MiB`.
- MEDIUM-1 (intent cap schema only): fixed — `BEFORE INSERT` trigger on `length(NEW.intent) > 4096`.
- MEDIUM-2 (hash collision rollback clarity): fixed — explicit `tx.rollback()` with explanatory comment.
- MEDIUM-3 (`as i64` cast on milliseconds): fixed — `i64::try_from` with `ClockOverflow` error.
- MEDIUM-4 (actor fields unchecked): noted as accepted risk; Phase 8 deferral documented with `VerifiedActor` stub.
- MEDIUM-5 (no write timeout): fixed — `tokio::time::timeout` + `with_limits` builder.
- LOW-1 (corpus rot): fixed — `desired_state_canonical.rs` round-trips actual `DesiredState` instances with known hashes.
- LOW-2 (migration numbering): fixed — resolved as `0004_snapshots_immutable.sql`; test `migration_sequence_contiguous` added.

No round 1 finding is re-raised below unless the fix itself introduces a new problem.

---

## Findings

---

### HIGH: SQLite `length()` is character-count, not byte-count — intent trigger cap is inconsistent with the Rust check

**Category:** Logic Flaws

**Trigger:** A caller constructs an `intent` string containing multibyte UTF-8 characters — for example, 2049 U+1F600 emojis (4 bytes each, 8196 bytes total). The Rust adapter checks `inputs.intent.len() > 4096` where `.len()` returns bytes. This check fires at 2049 emojis (8196 > 4096) and returns `WriteError::IntentTooLong`. However, if the Rust check is bypassed (raw SQL insert, test fixture, future code path that constructs `SnapshotInputs` from a pre-validated `intent`), the SQLite trigger fires on `WHEN length(NEW.intent) > 4096`. SQLite's `length()` on TEXT returns the number of characters (Unicode codepoints), not bytes. A 2049-emoji string has `length() = 2049`, which is under 4096, so the trigger does NOT fire. The trigger also does not fire for any multibyte-character intent up to 4096 codepoints — which could be up to 16384 bytes (if entirely 4-byte codepoints).

**Consequence:** The intent column can hold up to 16384 bytes (16 KiB) of multibyte content before the schema-layer trigger stops it. The round 1 HIGH finding about the schema not enforcing the cap was addressed, but the fix enforces a character-count cap, not a byte-count cap. The two enforcement points (Rust: bytes, SQLite: codepoints) diverge. Oversized rows can be inserted by any tool that bypasses the Rust adapter. Once inserted, the row is immutable and cannot be corrected.

**Design assumption violated:** The design states the trigger "enforces the 4 KiB intent length cap at the storage layer" and describes it as defence in depth. It assumes `length()` returns bytes. SQLite's `length()` function returns the number of characters for TEXT columns; byte-count requires `length(CAST(NEW.intent AS BLOB))`.

**Suggested mitigation:** Change the trigger condition from `WHEN length(NEW.intent) > 4096` to `WHEN length(CAST(NEW.intent AS BLOB)) > 4096`. `CAST(x AS BLOB)` converts TEXT to its UTF-8 byte representation in SQLite, and `length()` on BLOB returns the byte count. This makes the schema-layer and Rust-layer checks consistent.

---

### HIGH: `expected_version = Some(-1)` bypasses the root-creation sentinel and creates an undocumented code path

**Category:** Logic Flaws

**Trigger:** A caller calls `SnapshotWriter::write` with `expected_version: Some(-1)` on an instance that has never had a snapshot. The algorithm reads `MAX(config_version)` for the instance, which returns `NULL`, then converts that to `-1` (the sentinel "no rows exist"). The OCC check evaluates: `inputs.expected_version == Some(-1)` and `current_max == -1`, so `v != current_max` is false — no conflict. The caller proceeds as a root creator, computing `new_version = -1 + 1 = 0`. The INSERT succeeds. The caller has passed `Some(-1)` and gotten root-creation semantics, bypassing the stated invariant that "`None` is permitted ONLY for the very first snapshot of an instance (root creation by the system actor)."

**Consequence:** Any actor — including non-system actors — can root-create a snapshot by passing `Some(-1)` instead of `None`. The distinction between system-actor root creation (`None`) and any-actor root creation (`Some(-1)`) is not enforced. More subtly, future code that enforces "only the system actor may pass `None`" to gate root creation will be trivially bypassed by passing `Some(-1)`. The audit trail records the actor faithfully, but the actor-restriction invariant is not preserved.

**Design assumption violated:** The design states "`None` is permitted ONLY for the very first snapshot of an instance (root creation by the system actor)." The check only tests whether `current_max == -1` via numeric equality, not whether the caller's expectation semantics are `None`. `Some(-1)` satisfies the same numeric condition.

**Suggested mitigation:** Add an explicit guard: if `inputs.expected_version == Some(-1)`, return a new `WriteError::InvalidExpectedVersion { reason: "expected_version must not be -1; pass None for root creation" }`. The value `-1` is internal sentinel state; callers should never see or pass it as a version. Document that valid values for `Some(v)` are `v >= 0`.

---

### HIGH: `daemon_run_id` process-wide invariant is enforced by convention, not by type — multiple writers can diverge

**Category:** State Manipulation

**Trigger:** Two `SnapshotWriter` instances are constructed in the same process: `SnapshotWriter::new(pool.clone(), "local".into(), "ULID-A".into())` and `SnapshotWriter::new(pool.clone(), "local".into(), "ULID-B".into())`. Both are valid constructions; the API accepts any `String`. Snapshots written by the first writer have `daemon_run_id = "ULID-A"` and by the second have `daemon_run_id = "ULID-B"`. Both write to the same `caddy_instance_id`. The ordering invariant — that `(created_at_ms, daemon_run_id, created_at_monotonic_nanos)` is the correct cross-restart sort key — now has *two* different `daemon_run_id` values within a single daemon run, breaking the assumption that `daemon_run_id` uniquely identifies a process lifetime.

**Consequence:** Queries ordering by `(created_at_ms, daemon_run_id, created_at_monotonic_nanos)` group snapshots written in the same millisecond into two separate daemon-run buckets even though they were written by the same process. Phase 7 rollback traversal that sorts snapshots by this key will produce non-linear history within a single process lifetime. The `daemon_run_id` column can no longer be used to distinguish "before restart" from "after restart."

**Design assumption violated:** The design says "MUST be the same ULID for every writer in the same daemon process (typically stored in a process-wide `OnceLock<String>` initialised at daemon boot)." This is enforced by documentation and convention only. The `SnapshotWriter::new` constructor has no mechanism to prevent two different ULIDs from being passed.

**Suggested mitigation:** Expose `daemon_run_id` as a module-level function (`adapters::daemon_clock::run_id() -> &'static str`) backed by a `OnceLock<String>`, and remove the `daemon_run_id` parameter from `SnapshotWriter::new`. The constructor reads it directly from the module-level OnceLock. A separate `daemon_clock::init(run_id: String)` is called once at daemon boot (panicking if called twice via `OnceLock::set`). This makes it structurally impossible to construct a writer with the wrong run ID.

---

### HIGH: Constraint-name parsing of SQLite error strings is not contractually stable across SQLite versions

**Category:** Logic Flaws

**Trigger:** The revised design's step 12 parses the SQLite error message string to distinguish `snapshots.id` primary key collision from the `snapshots_config_version` unique index collision, and routes each to a different error variant (`Deduplicated`/`HashCollision` vs `VersionRace`). On SQLite 3.x, the error message for a UNIQUE constraint violation is `"UNIQUE constraint failed: <table>.<column>[, <table>.<column>]"` — this format is documented in SQLite source but is not part of any formal stability contract. If a future SQLite version changes the error string (e.g., to use a different separator, or to include the constraint name rather than the column names), the parser produces an incorrect match and routes the error to the wrong variant.

**Consequence:** A `config_version` collision that should return `VersionRace` instead falls through to the dedupe branch. The dedupe branch fetches a row by `id`, but the conflicting row has the same `config_version` for the same instance — not the same `id`. The fetch by id may return `None` (the conflicting row has a different id). The code then treats `None` as... what? The algorithm does not specify what happens if the fetched-by-id row is absent in the dedupe branch. Most likely this produces an unexpected `Sqlx` error or a panic. The critical invariant — distinguishing a retryable version race from a fatal hash collision — is broken whenever the error string format deviates from the expected pattern.

**Design assumption violated:** The design assumes SQLite's error string for UNIQUE constraint violations is stable and machine-parseable. It is a human-readable diagnostic string. SQLite's documented programmatic way to identify which constraint fired is via `sqlite3_vtab_conflict()` or checking `sqlite3_errmsg()`/`sqlite3_extended_errcode()` — neither of which is directly surfaced by sqlx in a structured form. The `sqlx` crate does not expose structured constraint-failure metadata.

**Suggested mitigation:** Instead of parsing error strings, restructure the INSERT so that the two constraints can never ambiguously collide. One approach: before the INSERT, do a targeted SELECT to check whether the id already exists (`SELECT 1 FROM snapshots WHERE id = ?`). If it does, enter the dedupe path without relying on constraint name parsing. The UNIQUE index on `(caddy_instance_id, config_version)` then only fires for the version-race case (the id check passed, but between the id-check and the INSERT, a concurrent writer took the same version — still unreachable with `BEGIN IMMEDIATE`, but now correctly identified if it ever fires). A secondary option is to add a SQLite user-defined function that surfaces `sqlite3_extended_errcode()` at the point of conflict, but that is considerably more complex.

---

### MEDIUM: `process_start()` `OnceLock<Instant>` can be read before initialization — design does not specify initialization order

**Category:** Race Conditions

**Trigger:** `SnapshotWriter::new` accepts a `daemon_run_id` string but reads the monotonic clock from `crate::daemon_clock::process_start().elapsed()`. The `daemon_clock::process_start()` function accesses a `OnceLock<Instant>`. If `SnapshotWriter::new` is called before `daemon_clock` is initialized (i.e., before the daemon boot sequence calls the initializer), `OnceLock::get()` returns `None`. The design does not show how `process_start()` handles an uninitialized state — it shows only `.elapsed()` on the result, which would require an `unwrap()` or `expect()`. Either the function panics (violating the "no `unwrap()` in production code" rule), or it returns a sentinel `Instant` — neither is specified.

**Consequence:** If the OnceLock is read before initialization and the code panics, the daemon crashes at the first snapshot write after an early-boot writer construction. If the code falls back to a sentinel (e.g., `Instant::now()`), every writer constructed before boot completion uses a different "process start" reference, producing inconsistent `created_at_monotonic_nanos` values within the same process.

**Design assumption violated:** The design says `process_start()` "returns the process-wide `Instant` captured at daemon boot" but does not specify what happens if it is accessed before that capture occurs. The design assumes initialization happens before any `SnapshotWriter` is used — this is a temporal dependency that the type system does not enforce.

**Suggested mitigation:** Either (a) panic with an explicit message (`"daemon_clock::init() must be called before SnapshotWriter can be used"`) so the failure is loud and clear during development, documented as "this is a programmer error, not a runtime error"; or (b) initialize `process_start` lazily to `Instant::now()` with a `warn!` log if called before explicit initialization. Option (a) is simpler and consistent with the project's "no silent shortcuts" rule. Either way, the behavior must be specified in the design.

---

### MEDIUM: Deduplication body-comparison is performed inside the `BEGIN IMMEDIATE` write lock

**Category:** Resource Exhaustion

**Trigger:** A caller writes a snapshot with a large `desired_state_json` (say, 8 MiB — within the 10 MiB cap). A second concurrent caller writes the byte-identical state. The second caller hits the UNIQUE constraint on `id`, enters the dedupe path at step 12.1, and then fetches the existing row by id (step 12.1.1) — an 8 MiB `SELECT *` — while holding the `BEGIN IMMEDIATE` write lock. The comparison at step 12.1.2 iterates over all 8 MiB bytes. All other writers waiting to acquire the write lock (via `PRAGMA busy_timeout`) are blocked for the duration of this comparison.

**Consequence:** Under frequent deduplication events with large snapshots (a realistic pattern during idempotent re-applies), every write attempt is serialized behind a large in-transaction memory comparison. With `busy_timeout = 5000 ms` and a 5-second `write_timeout`, a write attempting the IMMEDIATE lock during a slow dedupe comparison can time out, returning `WriteError::Timeout` to an innocent caller who submitted a fresh, non-duplicate write.

**Design assumption violated:** The design places the dedupe body-comparison inside the already-acquired `BEGIN IMMEDIATE` transaction. This was done to make the check-then-write atomic, but deduplication is a read operation that does not need to hold the write lock. The write lock is only needed for the INSERT path.

**Suggested mitigation:** Perform a preliminary `SELECT` for an existing row by `id` *before* acquiring the `BEGIN IMMEDIATE` lock. If a row with that id already exists, compare bodies and return `Deduplicated` or `HashCollision` without ever taking the write lock. Only proceed to `BEGIN IMMEDIATE` when no existing row is found. This is safe: the id is deterministic from the content, and if the same content is re-submitted between the preliminary check and the lock acquisition, the UNIQUE constraint will fire again — and the code can enter the dedupe path inside the transaction at that point. This two-phase approach keeps write-lock hold time minimal.

---

### MEDIUM: `by_id` fetch is not scoped to the caller's instance — returns cross-instance snapshots

**Category:** Data Exposure

**Trigger:** The `SnapshotFetcher` is constructed with `instance_id = "instance-A"`. A caller invokes `fetcher.by_id(some_id)` where `some_id` is the content-addressed id of a snapshot belonging to `instance-B`. The algorithm is `SELECT * FROM snapshots WHERE id = ?` with no `AND caddy_instance_id = ?` filter. The query succeeds and returns the instance-B snapshot, including its `desired_state_json`, actor, intent, and all other fields.

**Consequence:** A caller operating in the context of instance A can read any snapshot in the database by guessing or iterating SHA-256 hex ids. In V1 with `local` hard-coded as the only instance, this is a data integrity issue (instance-scoped APIs should not return out-of-scope data) rather than a cross-tenant security breach. However, since the architecture reserves `caddy_instance_id` explicitly for future multi-instance support (T3.1), this unscoped fetch will become a cross-tenant data exposure bug the moment T3.1 lands unless it is corrected before then. Given the immutable record, there is no way to delete or redact the leaked data after the fact.

**Design assumption violated:** The `SnapshotFetcher` holds `instance_id` but `by_id` does not use it, implying the assumption that id uniqueness across all instances is sufficient scoping. Content-addressed ids are globally unique (hash of content), but the `SnapshotFetcher.instance_id` field's purpose is to scope queries — using it inconsistently across methods creates an API surface where some operations are instance-scoped and others are not.

**Suggested mitigation:** Change `by_id` to `SELECT * FROM snapshots WHERE id = ? AND caddy_instance_id = ?`, binding `self.instance_id`. If a valid use case requires cross-instance lookup (e.g., integrity verification), provide a separate `by_id_global` method with explicit documentation of its scope.

---

### MEDIUM: `regen-snapshot-hashes` binary freezes the current canonicaliser output as "correct" without any validation gate

**Category:** Rollbacks

**Trigger:** A developer introduces a bug in `canonical_json.rs` (e.g., a key-sorting edge case with non-ASCII characters). The bug changes canonical output for some `DesiredState` instances. The `desired_state_canonical.rs` test fails, correctly detecting the regression. The developer, under time pressure or unfamiliarity with the test setup, runs `cargo run --bin regen-snapshot-hashes` to "fix" the test. The command regenerates `desired_state_hashes::FIXTURES` with the buggy hashes. The test now passes. The canonicaliser bug is silently baked in.

**Consequence:** All subsequent snapshots written with the broken canonicaliser produce hashes that differ from pre-bug snapshots for the same logical `DesiredState`. Deduplication fails — byte-identical logical states produce different SHA-256 ids. The content-addressing invariant is violated. Since snapshots are immutable and append-only, there is no path to correct the historical records. The `regen-snapshot-hashes` tool, intended as a maintenance aid, becomes the mechanism through which a bug is institutionalised. The regression is invisible because the test passes after regeneration.

**Design assumption violated:** The design presents `regen-snapshot-hashes` as a maintenance tool for legitimate canonicaliser changes (accompanied by a `CANONICAL_JSON_VERSION` bump). However, the tool has no way to distinguish a legitimate intentional change from a bug fix that should not be baked in. Running it is always safe in appearance; its output is always a valid fixture file.

**Suggested mitigation:** Add a `--require-version-bump` flag that the tool checks: if `CANONICAL_JSON_VERSION` has not been incremented from the value in the existing fixtures file, the tool refuses to regenerate and prints an explanatory error. Document in `core/README.md` that `regen-snapshot-hashes` must ONLY be run in conjunction with bumping `CANONICAL_JSON_VERSION` via an ADR-ratified change. Consider adding a CI gate that runs the test without regeneration and treats regeneration as a deliberate, code-reviewed step.

---

### MEDIUM: Offset-based pagination in `children_of` and `in_range` is unstable under concurrent inserts

**Category:** Eventual Consistency

**Trigger:** A caller fetches children of a parent snapshot using `Page { limit: 100, offset: 0 }` and receives rows 1–100. Between this call and the next call with `Page { limit: 100, offset: 100 }`, a concurrent writer inserts a new child snapshot for the same parent. The new child has an `id` (SHA-256 hex) that sorts alphabetically earlier than some of the rows the caller already received. The ORDER BY clause is `created_at ASC, id ASC`. The new child's `created_at` is the current wall-clock time, which places it at or after the already-received rows (likely at the end). However, if the new child has a `created_at` equal to a row the caller already received (two writes in the same second), the `id ASC` tiebreaker places it before some already-seen rows. The second page call with `offset: 100` skips the new child.

**Consequence:** A paginator walking all children of a snapshot misses rows inserted during the traversal. This is a standard offset-based pagination hazard. For the Phase 5 use case (snapshot history traversal, rollback chain building in Phase 7), a missed snapshot means the rollback chain is incomplete. Phase 7 code that assumes `parent_chain` returns the full lineage will silently miss snapshots inserted concurrently.

**Design assumption violated:** The design adds the `id ASC` tiebreaker (specifically to "stabilise pagination for snapshots written in the same wall-clock second"), but this stabilises ordering only for already-present rows. It does not address new inserts that shift offsets. The design's description of `id ASC` as making pagination stable conflates "deterministic order for a fixed dataset" with "stable cursor across a changing dataset."

**Suggested mitigation:** Document explicitly that offset-based pagination is subject to insertion skew and is not safe for snapshot chain reconstruction. For Phase 7 rollback chain building, use the `parent_id` pointer traversal (walk `parent_id` links) rather than paginated `children_of`, since parent pointers are immutable and form a stable linked list regardless of concurrent inserts. For the general list API, add a note to `SnapshotFetcher` docs that callers requiring completeness guarantees must use keyset pagination (e.g., `WHERE created_at > last_seen OR (created_at = last_seen AND id > last_id)`), which Phase 5 does not implement but should note as a future improvement.

---

### LOW: `ClockOverflow` error variant is unreachable in practice but the unreachability obscures actual overflow risk

**Category:** Logic Flaws

**Trigger:** The design adds `ClockOverflow(u128)` as a `WriteError` variant for the case where wall-clock milliseconds exceed `i64::MAX`. This requires a system clock reading of approximately 292 million years in the future (year ~292,278,994). The error variant is structurally correct but effectively dead code. The design's hint notes this ("reachable only above ~292 million years — accepted").

**Consequence:** This is not a correctness issue on its own. However, the presence of `ClockOverflow` in the error enum, paired with the documentation that it is "essentially dead code," creates a trap for future code reviewers and auditors: they will see an error variant that is never tested (no test can trigger it with a real system clock), never observed in production, and never returned. Unused error variants in a closed enum discourage exhaustive pattern matching. A future match statement that handles `WriteError` will need a `ClockOverflow` arm that can never fire, which is either `unreachable!()` (a panic in dead code) or silently ignored. Meanwhile, the *actual* overflow concern — `u64::try_from(nanos).unwrap_or(u64::MAX)` for monotonic nanos — is accepted with a comment but produces a sentinel `u64::MAX` rather than a typed error, which is inconsistent: the milliseconds path returns `Err(ClockOverflow)`, but the nanos path silently saturates.

**Design assumption violated:** The design is internally inconsistent: it adds a typed error for an overflow that cannot happen in practice while silently saturating an overflow that could theoretically happen first (after 584 years of process uptime vs 292 million years of wall clock). If overflow checking is worth a variant, both paths should either return typed errors or both should saturate silently with a comment.

**Suggested mitigation:** Either (a) remove `ClockOverflow` from the error enum and replace both paths with saturating arithmetic and a `warn!()` tracing event (consistent: neither returns `Err`), or (b) keep `ClockOverflow` and replace `unwrap_or(u64::MAX)` on the nanos path with a `ClockOverflow` return (consistent: both return `Err`). Option (a) is simpler and removes a dead code variant. If option (b) is chosen, add a note to the `ClockOverflow` variant's doc comment that the milliseconds variant fires at year ~292M and the nanos variant fires at process uptime ~584 years, both accepted as unreachable in practice but preserved for completeness.

---

### LOW: Architecture §6.5 schema is missing the `daemon_run_id` column added by migration 0003

**Category:** Orphaned Data

**Trigger:** The architecture document at §6.5 lists the `snapshots` table schema. The revised design adds a `daemon_run_id TEXT NOT NULL DEFAULT ''` column via `0003_snapshot_monotonic_nanos.sql`. The design's slice 5.3 notes that "Architecture §6.5 SHOULD be updated in the same commit to keep the table description authoritative" (open question 6) for `created_at_monotonic_nanos` and `canonical_json_version`, but does not explicitly mention `daemon_run_id`. The architecture doc does not include `daemon_run_id` in the schema block.

**Consequence:** The architecture document, which is the "canonical architecture reference" per its own header, now describes a schema that does not match the live database. Future implementers reading §6.5 will not know `daemon_run_id` exists, will not include it in queries, and will not account for it in the ordering rules. Phase 6 audit writers reading the architecture doc to understand the snapshot schema will construct incorrect queries. This is an orphaned documentation state — the column exists in the database but is invisible in the authoritative spec.

**Design assumption violated:** The design notes open question 6 for two of the three new columns but omits `daemon_run_id` from the "architecture §6.5 must be updated" list, even though it is the most semantically significant new column (it defines the cross-restart ordering rule).

**Suggested mitigation:** Add `daemon_run_id` to the open question 6 resolution note in slice 5.3, and update `docs/architecture/architecture.md` §6.5 to include all three new columns with the same quality of inline documentation as `canonical_json_version` currently has in the revised schema.

---

## Summary

**Critical:** 0  **High:** 4  **Medium:** 4  **Low:** 2

**Top concern:** The `length()` vs byte-count mismatch in the intent-cap trigger (HIGH-1) is the highest-consequence finding because it creates a permanent, uncorrectable inconsistency between the Rust and schema enforcement layers — any intent that slips past the trigger is immutably stored and cannot be deleted. The HIGH-2 finding (`Some(-1)` bypassing root-creation semantics) and HIGH-3 (multiple `SnapshotWriter` instances with different `daemon_run_id` values) both corrupt invariants that downstream phases (Phase 7 rollback chain, Phase 6 audit) silently depend on.

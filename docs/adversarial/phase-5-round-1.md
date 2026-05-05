# Adversarial Review — Phase 05 — Round 1

**Design summary:** Phase 5 implements a content-addressed, append-only snapshot store for Trilithon's desired-state history. The `SnapshotWriter` adapter serialises a `DesiredState` to canonical JSON, SHA-256 hashes it to form a primary key, deduplicates against existing rows, enforces parent linkage and monotonically increasing `config_version` per Caddy instance, and persists the record in one SQLite transaction. Supporting pieces include the canonical JSON serialiser, the SHA-256 helper, two schema migrations adding new columns and immutability triggers, fetch operations, and property tests.

**Prior rounds:** None — this is round 1.

---

## Findings

---

### CRITICAL: `config_version` self-assignment breaks ADR-0012 optimistic concurrency contract

**Category:** Logic Flaws

**Trigger:** A caller submits a mutation with `expected_version = 5` (the version it observed). At Phase 5 the `SnapshotWriter` ignores `expected_version` entirely. It reads `SELECT MAX(config_version)` from the database and assigns `MAX + 1`. If `current_version` is already 7 (because two other actors have written since the caller fetched state), the writer silently accepts and commits at version 8. The caller intended to guard against stale state; the guard never executes.

**Consequence:** The ADR-0012 contract — "If `expected_version != current_version`, fail with a typed `ConflictError`" — is structurally absent from `SnapshotWriter`. Two actors mutating against the same base state both succeed. Hazard H8 ("silent data loss") materialises exactly as ADR-0012 was designed to prevent. The current design provides snapshot deduplication (same hash → same row) but not version-based conflict detection, which are different things: two actors computing different desired states from the same base will each get a new row, both succeeding.

**Design assumption violated:** The design argues in slice 5.5 that "the snapshot writer assigns the version monotonically; callers do NOT pass `config_version` because it would violate single-source-of-truth." This directly contradicts ADR-0012, which states callers SHALL carry `expected_version` and the mutation SHALL fail if `expected_version != current_version`. The design's justification for removing caller-supplied version conflates the *assignment* of the next version (which can be internal) with the *validation* of whether the caller is operating on current state (which requires the caller's observed version as input). The snapshot writer can assign the next version internally while still accepting and validating `expected_version` from the caller.

**Suggested mitigation:** Add `expected_version: Option<i64>` to `SnapshotInputs`. When `Some(ev)`, after the `SELECT MAX(config_version)` step, compare: if `ev != current_max`, return a new `WriteError::ConflictError { expected: ev, current: current_max }`. `None` is acceptable only for the very first snapshot per instance (root creation by the system actor), not for human or LLM mutations. Phase 6 or 7 must thread `expected_version` from the HTTP handler through the mutation pipeline into `SnapshotInputs`.

---

### CRITICAL: TOCTOU race on `config_version` assignment yields opaque errors under concurrent writers

**Category:** Race Conditions

**Trigger:** Two async tasks both call `SnapshotWriter::write` for the same `caddy_instance_id` concurrently. Task A and Task B each execute `SELECT COALESCE(MAX(config_version), -1)` and both read `current_max = 4`. Both compute `new_version = 5`. Task A's `INSERT` succeeds and commits. Task B's `INSERT` fails with SQLite error `UNIQUE constraint failed: snapshots.caddy_instance_id, snapshots.config_version`.

**Consequence:** Task B receives `WriteError::Sqlx { source: sqlx::Error }` — an opaque storage error. The design's `NonMonotonicVersion` error variant is never returned in this path (it is only returned by the explicit `> max(config_version)` check mentioned in the design description but not shown in the algorithm — and that check also cannot fire because both writers computed the same value). The caller cannot distinguish a version-collision retry scenario from a schema violation or disk error. SQLite's `busy_timeout` defaults apply separately from WAL write serialisation; the constraint fires before any lock timeout. Task B's write is silently lost to the caller.

**Design assumption violated:** The design states "Architecture §6.5 unique index on `(caddy_instance_id, config_version)` enforces this at the database level too" — treating the unique index as a safety net rather than handling the constraint violation as a typed error. Step 9 of the algorithm handles the `id` constraint violation for deduplication but does not handle the `config_version` constraint violation at all. The algorithm must distinguish which constraint fired.

**Suggested mitigation:** After catching a `UNIQUE constraint failed` error in step 9, inspect the constraint name (SQLite exposes this in the error string). If it is the `snapshots_config_version` index, retry the entire transaction from step 6 (re-read MAX, recompute, retry insert) up to N times before returning a typed `WriteError::VersionConflict`. Alternatively, hold the SQLite write lock (via `BEGIN IMMEDIATE` instead of `BEGIN`) across the MAX-read and INSERT, making the entire check-then-write atomic at the database level.

---

### HIGH: Cross-instance parent linkage is not prevented

**Category:** State Manipulation

**Trigger:** A caller calls `SnapshotWriter::write` with `instance_id = "instance-A"` and `inputs.parent_id = Some(SnapshotId("abc123..."))` where `abc123` is a valid snapshot id that belongs to `instance-B`. The parent lookup is `SELECT 1 FROM snapshots WHERE id = ?` — it does not filter by `caddy_instance_id`. The lookup succeeds. The INSERT succeeds. The snapshot for instance A now has a parent pointer into instance B's history chain.

**Consequence:** The parent-chain traversal logic (`children_of`, `parent_chain`) for instance A encounters a parent snapshot belonging to instance B. Queries like `children_of(parent_id)` filter by `caddy_instance_id` (the design shows `WHERE parent_id = ? AND caddy_instance_id = ?`), so the chain breaks — the parent exists in the DB but is invisible to instance A's chain query. Rollback (Phase 7) traversing the parent chain for instance A would silently dead-end. The "root snapshot has NULL parent" invariant test does not cover this case.

**Design assumption violated:** The design assumes `parent_id` refers to a snapshot within the same instance. The parent lookup does not enforce this boundary.

**Suggested mitigation:** Change the parent existence check from `SELECT 1 FROM snapshots WHERE id = ?` to `SELECT 1 FROM snapshots WHERE id = ? AND caddy_instance_id = ?`, binding `self.instance_id` as the second parameter. Add a test `tests::parent_from_different_instance_is_rejected`.

---

### HIGH: Ambiguous unique-constraint dispatch — deduplication path can silently absorb `config_version` collisions

**Category:** Logic Flaws

**Trigger:** A `config_version` collision fires (two concurrent writers, both computed version 5). The resulting SQLite error is `UNIQUE constraint failed`. The algorithm at step 9 checks for "a unique-constraint violation on `id`" — but the error message and error kind from sqlx do not automatically identify which constraint was violated. If the implementation naively catches any `UNIQUE constraint failed` and enters the deduplication branch (fetch existing row, compare bytes), it fetches a different row (the one that holds `config_version = 5` for this instance), finds that `desired_state_json` differs, and returns `WriteError::HashCollision` — incorrectly reporting a hash collision when the actual problem is a version race.

**Consequence:** The operator sees `HashCollision` for an event that is actually a concurrent-write version race. If the implementation instead returns `Sqlx` for the version collision, the caller gets an opaque error for a retryable condition. Either path produces wrong diagnostics and wrong retry behaviour.

**Design assumption violated:** The algorithm assumes the only unique-constraint violation that can occur is on the `id` column. The table has two unique constraints (`PRIMARY KEY` on `id` and `UNIQUE INDEX` on `(caddy_instance_id, config_version)`). Step 9 does not handle the second.

**Suggested mitigation:** Parse the SQLite error string to extract the constraint name before dispatching. SQLite error messages include the constraint name (e.g., `UNIQUE constraint failed: snapshots.id` vs `UNIQUE constraint failed: snapshots.caddy_instance_id, snapshots.config_version`). Route each case to its own typed error variant.

---

### HIGH: `in_range` and `children_of` load unbounded result sets into memory including full `desired_state_json`

**Category:** Resource Exhaustion

**Trigger:** A `SnapshotFetcher::in_range` query is issued with `from = 0` (epoch) and `to = now`. In a daemon that has been running for a year with a busy operator, this matches thousands of snapshots. Each row includes `desired_state_json TEXT NOT NULL`, which for a large Caddy config can be tens to hundreds of kilobytes. The query returns `Vec<Snapshot>` with all rows fully materialised.

**Consequence:** A single `in_range` call can load hundreds of megabytes into the async runtime's heap. The design applies no `LIMIT`, no cursor/pagination, and no size cap on `desired_state_json`. SQLite itself has no row-count limit for `SELECT`. The `children_of` function has the same problem: a high-fan-out snapshot (e.g., after an import that spawned many children) loads all descendants in one call.

**Design assumption violated:** The design treats these as simple fetch helpers but the underlying data is append-only and unbounded. The architecture notes "The SQLite database grows monotonically" (ADR-0009 consequences) but the fetch design does not account for this in its API contract.

**Suggested mitigation:** Add `limit: u32` and `offset: u32` parameters to `in_range` and `children_of`. Enforce a maximum page size (e.g., 500 rows) in the adapter. For list-style APIs in later phases, expose pagination to callers. At minimum add a design-time constant `MAX_FETCH_ROWS` and document that callers must paginate.

---

### HIGH: Monotonic nanos resets to near-zero after daemon restart, violating any cross-restart ordering

**Category:** Eventual Consistency

**Trigger:** The daemon runs for 30 minutes, writing snapshots with `created_at_monotonic_nanos` values from ~0 to ~1.8 × 10^12 (nanoseconds). The daemon restarts. The `OnceLock` for the process-start `Instant` is re-initialised. The next snapshot written 1 second after restart has `created_at_monotonic_nanos = ~1 × 10^9`. This is numerically less than any snapshot written more than 1 second into the previous daemon run.

**Consequence:** Any code that queries snapshots and orders by `created_at_monotonic_nanos` without also checking `created_at_ms` produces incorrect ordering across restarts. The architecture (§6.5) states "It MUST NOT be used to order events across daemon restarts" — but this constraint is not enforced at the schema or query level. Nothing prevents a future query or a downstream consumer from ordering by `created_at_monotonic_nanos` alone. The column name does not communicate this limitation. Phase 6 audit consumers and Phase 7 rollback traversal will both see the snapshots table in time-sorted queries.

**Design assumption violated:** The design relies on documentation (architecture §6.5 note) as the sole guard against misuse of `created_at_monotonic_nanos`. There is no schema or type-level enforcement.

**Suggested mitigation:** Add a `daemon_run_id` column (a ULID or UUID generated at daemon boot, stored in the `OnceLock` alongside the start `Instant`) to every snapshot row. Ordering queries that need true cross-restart ordering should sort by `(created_at_ms, daemon_run_id, created_at_monotonic_nanos)`. This makes the ordering semantics explicit and queryable rather than implicit. At minimum, add a comment on the column in the migration AND add a clippy lint or module-level doc warning that queries on this column must always be combined with `created_at_ms`.

---

### HIGH: `desired_state_json` has no size cap — snapshot rows can be arbitrarily large

**Category:** Resource Exhaustion

**Trigger:** A `DesiredState` is constructed with thousands of routes, each with large handler configurations. The canonical JSON serialiser produces a multi-megabyte byte vector. `to_canonical_bytes` returns it, `content_address` hashes it, and the INSERT stores the full text in `desired_state_json TEXT NOT NULL` — unlimited by any schema constraint, validation in `SnapshotInputs`, or the writer's algorithm.

**Consequence:** Individual snapshot rows could be tens of megabytes. SQLite supports this (TEXT has a configurable max of 1 GiB by default), but each `SELECT * FROM snapshots` anywhere in the fetcher materialises the full blob. A single large snapshot combined with a `children_of` call returning 50 children, each large, exhausts heap in the adapter process. Backup, export (T2.9), and chain-verification (ADR-0009) all read `desired_state_json` and are affected.

**Design assumption violated:** The design caps only `intent` at 4 KiB. It assumes `desired_state_json` is manageable in size. No policy or size limit is stated.

**Suggested mitigation:** Add a configurable `max_desired_state_bytes: usize` to `SnapshotWriter::new`. Before the INSERT, check `bytes.len() <= max_desired_state_bytes` and return a new `WriteError::StateTooLarge { len, max }`. A reasonable default for V1 might be 10 MiB. Separately, fetch queries should select columns explicitly (e.g., omit `desired_state_json` from list queries and only include it in `by_id` lookups).

---

### MEDIUM: `intent` length cap enforced in code only — schema allows unlimited storage

**Category:** State Manipulation

**Trigger:** A future migration, a raw SQL import, a test fixture, or a direct SQLite write (SQLite has no application-level firewall) inserts a snapshot row with `intent` longer than 4096 bytes. The immutability triggers block UPDATE and DELETE but not INSERT from outside the adapter.

**Consequence:** The Rust-layer cap is advisory — any row in the database that bypasses the adapter has an uncapped `intent`. When these rows are fetched and deserialised into `Snapshot` structs, the `intent: String` field silently holds oversized content. If Phase 6 or beyond copies `intent` into audit rows or surfaces it to the API, the oversized content propagates. The invariant is fragile because it is not enforced at the storage boundary.

**Design assumption violated:** The design treats the 4 KiB cap as a `WriteError::IntentTooLong` check at the adapter layer only. The phase reference says "length-bounded at 4 KiB" without qualifying "in code only."

**Suggested mitigation:** Add a `CHECK (length(intent) <= 4096)` constraint to the `snapshots` table in the migration. This enforces the invariant at the database level. The Rust-layer check then becomes a fast-path that avoids a round-trip on invalid input, not the only enforcement.

---

### MEDIUM: Hash collision handling path is structurally incorrect — rollback after a completed INSERT is unnecessary

**Category:** Logic Flaws

**Trigger:** Two different byte sequences collide on their SHA-256 digest (theoretical but the code must be correct). The first write inserts row A with `id = H`. The second write calls `write`, computes `id = H`, attempts INSERT, receives `UNIQUE constraint failed` on `id`. The design then says: "Fetch existing row, compare bytes. Unequal → rollback and return `HashCollision`."

**Consequence:** The rollback is a no-op — the INSERT already failed, so nothing was written in this transaction. The transaction holds no pending write that needs rolling back. Rolling back is safe (it does nothing) but semantically misleading in the code. More importantly, the `INSERT` failure does not guarantee that the transaction is in a clean state if prior steps (parent lookup, MAX query) had side effects. In SQLite, a failed statement inside a transaction does not automatically abort the transaction (unlike `RAISE(ABORT)` triggers). The transaction is still open. The design says "rollback and return `HashCollision`" — the rollback is correct to execute but the comment suggesting it is rolling back the INSERT specifically is wrong, which will confuse implementers.

**Design assumption violated:** The design implies the rollback is undoing the INSERT. In SQLite with the WAL journal, a failed `INSERT` statement leaves the transaction open but unmodified; the rollback undoes any earlier statements in the same transaction (parent lookup reads — no-op since reads have no effect). The real risk is that the HashCollision path might accidentally omit the rollback (`tx.rollback()`) and then drop `tx`, which in sqlx causes an implicit rollback — but if the code doesn't call rollback explicitly, the connection returns to the pool in a broken state in some sqlx versions.

**Suggested mitigation:** Explicitly call `tx.rollback().await?` before returning `HashCollision` regardless, and add a code comment explaining why: "rollback here does not undo the INSERT (which already failed), but it ensures the transaction is cleanly closed and the connection returns to pool in a valid state." Add a test that verifies the pool is still usable after a hash-collision return.

---

### MEDIUM: `created_at_ms` cast from `u128` to `i64` silently truncates on overflow

**Category:** Logic Flaws

**Trigger:** The design shows: `let created_at_ms = now_st.as_millis() as i64;`. `Duration::as_millis()` returns `u128`. As of 2026, `SystemTime::now().duration_since(UNIX_EPOCH)` returns approximately 1.7 × 10^12 milliseconds, which fits in `i64` (max ~9.2 × 10^18). However, the `as` cast in Rust is a truncating cast — if for any reason `as_millis()` returns a value exceeding `i64::MAX`, the cast wraps silently to a large negative number rather than panicking or erroring.

**Consequence:** A snapshot with a negative `created_at_ms` is inserted, passes all current validation, is persisted with an immutable record (no UPDATE possible), and then permanently corrupts any time-range query that uses `created_at_ms`. The value cannot be corrected. This cannot happen with correct system clocks in any realistic near-future scenario, but the design has a production `unwrap_or_default()` nearby (`duration_since(UNIX_EPOCH).unwrap_or_default()`) suggesting the author is already thinking about clock anomalies.

**Design assumption violated:** The design uses a silent truncating cast where a checked conversion or an explicit bound check would make the invariant explicit.

**Suggested mitigation:** Replace `as i64` with `i64::try_from(now_st.as_millis()).unwrap_or(i64::MAX)` or return a `WriteError::ClockError` if the conversion fails. This is cheap, makes the invariant explicit, and prevents corrupt timestamps from being written.

---

### MEDIUM: `SnapshotInputs` actor fields are unchecked strings — writer stores unvalidated actor provenance

**Category:** Authentication & Authorization

**Trigger:** Any code that constructs a `SnapshotInputs` can set `actor_kind: ActorKind::User` and `actor_id: "admin"` regardless of the actual authenticated session. The `SnapshotWriter::write` method stores these fields verbatim. There is no cross-reference to a session table, token table, or any auth context.

**Consequence:** A bug (or future developer shortcut) in the call path that constructs `SnapshotInputs` causes a snapshot to be permanently recorded with fabricated actor attribution. Since snapshots are immutable (no UPDATE), the false attribution cannot be corrected. The audit trail's forensic value depends on `actor_id` being authoritative; if it is caller-supplied with no validation, any code path that reaches the writer can rewrite history as any actor.

**Design assumption violated:** The design says "actor_kind / actor_id are supplied by the caller" without requiring any validation against a live auth context. The phase is not responsible for auth (Phase 8 handles sessions), but the writer accepts the fields without any type-level guarantee they were authenticated.

**Suggested mitigation:** Introduce a `VerifiedActor` newtype in `core` that can only be constructed by the auth adapter (Phase 8). Replace `actor_kind: ActorKind` and `actor_id: String` in `SnapshotInputs` with `actor: VerifiedActor`. This makes it structurally impossible for a caller to fabricate actor attribution without going through the auth layer. Phase 5 ships the newtype stub; Phase 8 fills it in.

---

### MEDIUM: No timeout on SQLite transaction — writer hangs indefinitely under WAL pressure

**Category:** Timeouts & Retries

**Trigger:** A SQLite WAL checkpoint is running (triggered by the adapter, the OS, or a backup tool), or another long-running transaction holds the write lock. `SnapshotWriter::write` calls `pool.begin()`, which acquires a write transaction. SQLite's default `busy_timeout` is 0 (returns `SQLITE_BUSY` immediately) or is set by the pool's connection options. If `busy_timeout` is not configured, `pool.begin()` returns an error immediately — which is a separate problem. If `busy_timeout` is set to a long value (common to avoid `SQLITE_BUSY`), the writer blocks indefinitely.

**Consequence:** The async Tokio task executing `write` is blocked on a SQLite mutex. Since the pool has a finite connection count, enough blocked tasks can exhaust the pool. A sustained WAL checkpoint or a misbehaving backup process starves all snapshot writes. The design specifies no timeout on the transaction or its constituent queries.

**Design assumption violated:** The design says "single SQLite transaction" without specifying connection options, `busy_timeout`, or per-query timeout. The architecture notes SQLite WAL mode (§6 generally) but does not specify busy timeout policy.

**Suggested mitigation:** Add a `write_timeout: Duration` parameter to `SnapshotWriter::new` (default: 5 seconds). Wrap the transaction in `tokio::time::timeout(write_timeout, async { ... }).await.map_err(|_| WriteError::Timeout)?`. Additionally, specify that the SQLite pool MUST be configured with `pragma busy_timeout = 5000` at connection setup.

---

### LOW: Committed fixture corpus (50 pairs) will rot as `DesiredState` evolves

**Category:** Orphaned Data

**Trigger:** Slice 5.7 commits 50 JSON fixture pairs under `crates/core/tests/fixtures/canonical_corpus/`. The fixtures represent `serde_json::Value` pairs, not `DesiredState` structs. However, `to_canonical_bytes` calls `serde_json::to_value(value)?` first. If `DesiredState` gains a new field with a `#[serde(rename)]` or a custom serialiser in Phase 6+, the canonical form of existing `DesiredState` instances changes, but the fixture files do not change. The corpus test (`corpus_pairs_hash_identically`) reads the files as `serde_json::Value` directly (bypassing `DesiredState` serialisation), so it continues to pass. Meanwhile, real `DesiredState` instances no longer hash to the fixture values.

**Consequence:** The corpus test passes forever even after the canonical form drifts. The 50-state corpus becomes a regression test that tests `value_to_canonical_bytes` in isolation but no longer validates that actual `DesiredState` objects produce stable, expected hashes. The property of the corpus is silently weakened.

**Design assumption violated:** The corpus is presented as validating "canonicalisation of desired states" but the test reads raw JSON, not `DesiredState` objects. The disconnect means the corpus tests the lower-level `value_to_canonical_bytes` only.

**Suggested mitigation:** Add a separate integration test corpus that round-trips actual `DesiredState` fixtures through `to_canonical_bytes` and asserts known-good hashes. These fixtures should be generated by a helper script that snapshots the current canonical form of known states. When `DesiredState` changes, the script is re-run and the expected hashes are updated deliberately — making the drift visible in the diff rather than silent.

---

### LOW: Migration numbering discrepancy between phase reference and TODO is unresolved and could cause production migration failures

**Category:** Rollbacks

**Trigger:** The phase reference names the immutability migration `0002_snapshots_immutable.sql`. The TODO reconciles this to `0004_snapshots_immutable.sql` and documents the conflict in open question 7. If any tooling, CI script, documentation, or operator runbook references `0002_snapshots_immutable.sql` by name (the phase reference being the primary document used during code review), the wrong migration file is expected. An operator running a migration playbook derived from the phase reference applies them out of order.

**Consequence:** In the worst case, an operator running migrations manually on a production database applies `0002_snapshots_immutable.sql` before `0003_snapshot_monotonic_nanos.sql`, which references columns that don't exist yet. The trigger creation in `0004` on a schema without the new columns is also ordered-sensitive in SQLite (triggers reference column names at creation time). A wrong order produces a permanent schema inconsistency that cannot be fixed without a full database rebuild (since snapshots are immutable and the schema is live).

**Design assumption violated:** Open questions 6 and 7 are flagged but deferred to "before Phase 6." The TODO's phase exit checklist requires them to be resolved, but there is no enforcement mechanism — no test checks that the migration file names match the phase reference.

**Suggested mitigation:** Add a CI test that reads the migration directory and asserts the sequence `0001`, `0002`, `0003`, `0004` all exist and that no gaps or duplicates are present. Resolve the phase reference naming discrepancy before this phase exits by updating the phase reference to use the reconciled numbering, not just the TODO.

---

## Summary

**Critical:** 2  **High:** 5  **Medium:** 4  **Low:** 2

**Top concern:** The `SnapshotWriter` assigns `config_version` internally via `SELECT MAX + 1` and never validates the caller's `expected_version` — this structurally removes the optimistic concurrency guard that ADR-0012 mandates, allowing concurrent actors to overwrite each other's state without a conflict error. This is not a detail or an edge case; it is the primary safety mechanism for Trilithon's mutation pipeline.

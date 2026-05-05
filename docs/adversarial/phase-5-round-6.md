# Adversarial Review — Phase 05 — Round 6

**Design summary:** Phase 05 implements a content-addressed, append-only snapshot store in SQLite using a three-layer Rust workspace. Writers use optimistic concurrency control with SHA-256 content addressing, a pre-transaction dedup check, and a BEGIN IMMEDIATE transaction with a post-insert row-existence dispatch to distinguish inserts from dedup/race outcomes.

**Prior rounds:** 5 prior rounds addressed — all listed findings confirmed resolved. No re-raises below.

---

## Findings

### [HIGH] Step-12 version overflow silently assigns a negative config_version

**Category:** Logic Flaws

**Trigger:** An instance accumulates `i64::MAX` snapshots (or, more practically, a bug elsewhere writes a row with `config_version = i64::MAX` directly into the DB — e.g., a restore script or a future migration that seeds data). Step 10 reads `current_max = i64::MAX`. Step 12 computes `new_version = current_max + 1`. In Rust, `i64::MAX + 1` in release mode wraps to `i64::MIN` (wrapping arithmetic) or panics in debug mode. If the column is declared as `INTEGER` in SQLite and the value is stored, it silently inserts a row with `config_version = -9223372036854775808`. The OCC check at step 11 then accepts `expected_version = Some(-9223372036854775808)` from a caller (step 1 only rejects `v < 0` on the *input* side, not on the internally computed side), meaning future callers must pass a negative expected_version that step 1 will reject as `InvalidExpectedVersion`.

**Consequence:** The instance is permanently wedged. No caller can ever pass a valid `expected_version` again because step 1 rejects all negative values, but step 10 returns a negative `current_max`. The instance's snapshot chain is unrecoverably stuck.

**Design assumption violated:** The design assumes `current_max + 1` is always representable as a non-negative `i64`. It provides no overflow guard at step 12, and step 1's validation only covers caller-supplied values, not the internally computed `new_version`.

**Suggested mitigation:** Add an explicit overflow check at step 12: if `current_max == i64::MAX`, return a new `WriteError::VersionOverflow` variant. This is the same pattern used for `created_at_ms` overflow (saturating-clamp + warn), applied to `config_version`.

---

### [HIGH] Migration trigger atomicity gap — `sqlx::migrate!` records 0004 as applied even if the trigger DDL was partially executed

**Category:** Logic Flaws

**Trigger:** `sqlx::migrate!` runs each migration in a transaction and records the migration version in `_sqlx_migrations` on commit. However, SQLite DDL statements (`CREATE TRIGGER`) are transactional — they are rolled back on transaction failure. The atomicity gap is subtler: if migration 0004 contains multiple DDL statements (three `CREATE TRIGGER` statements) and the process is killed between the second and third `CREATE TRIGGER`, the transaction was never committed, so none of them are recorded. On restart, sqlx re-applies 0004 from scratch and all three triggers are created. This is correct.

The real gap: if a future operator uses `sqlite3` CLI to manually apply a partial fix and then manually updates `_sqlx_migrations`, or if a bug in sqlx causes it to mark a migration done before the DDL commits (not currently known but possible in edge cases), the `snapshots_intent_length_cap` trigger may be absent while `snapshots_no_update` and `snapshots_no_delete` are present. The immutability triggers are present; the intent-length cap is not. The Rust adapter's step-2 intent check still fires, but external SQL writers can insert oversize intents bypassing the schema-layer defence.

More concretely: the design has no startup verification that all three expected triggers exist. A deployment that lost only the intent-length trigger would pass all existing tests (which check the trigger fires for the specific names) and would only be caught if a test explicitly verifies all three are present by name.

**Consequence:** External SQL clients can insert snapshots with `intent` longer than 4096 bytes, violating the schema invariant documented in ADR-0009. The Rust adapter still enforces the limit, but the "defence in depth" rationale for the trigger is silently absent.

**Design assumption violated:** The design assumes migration 0004 is either fully applied or not applied. It has no startup verification that the specific trigger names it relies on are present in `sqlite_master`.

**Suggested mitigation:** Add a startup integrity check (run once in `SnapshotWriter::new` or a pool-setup helper): `SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='snapshots'` — assert the result contains all three expected trigger names (`snapshots_no_update`, `snapshots_no_delete`, `snapshots_intent_length_cap`). Return `StorageError` if any are absent. Cost: one query at startup.

---

### [MEDIUM] `write_timeout` covers steps 1–16 including the step-7 full-body fetch, creating a starvation loop for large-config dedup paths

**Category:** Timeouts & Retries

**Trigger:** Step 7 performs a full body fetch when `json_len == bytes.len()`. For a 10 MiB state blob, this fetch could take several hundred milliseconds on a loaded system. The same `write_timeout` (default 5s) is the outer `tokio::time::timeout` wrapping all steps. If step 7 consumes 4.8s (slow disk + large payload), step 8's `BEGIN IMMEDIATE` gets 200ms. SQLite's busy-timeout for the WAL-mode IMMEDIATE lock is separate from Tokio's timeout; if the busy handler fires and consumes the remaining 200ms, the outer Tokio timeout fires, returning `Timeout(5s)` to the caller. The caller retries with the same 5s timeout and hits the same budget starvation loop if the step-7 body is consistently slow.

The doc comment says "increase `write_timeout` when `max_desired_state_bytes` is large" — but callers who read only the type signature see `write_timeout: Duration` with no guidance on how to scale it relative to state size.

**Consequence:** Callers on loaded systems with large state blobs see spurious `Timeout` errors not obviously related to state size. No data corruption, but the failure mode is opaque.

**Design assumption violated:** The design assumes the 5s default timeout is sufficient when `max_desired_state_bytes` is at its default (10 MiB). This may be true on development hardware but not under I/O pressure.

**Suggested mitigation:** At minimum, add a construction-time assertion: `if write_timeout < min_expected_for_max_bytes(max_desired_state_bytes) { tracing::warn!(...) }` with a documented formula (e.g., 1ms per KiB + 1s transaction budget). Or skip the step-7 full-body fetch entirely for the write path and rely solely on step-14 post-insert dispatch to handle dedup — this eliminates the timeout-starvation source entirely at the cost of one extra body fetch in the rare collision case.

---

### [MEDIUM] `children_of` sorts by `(created_at_ms, id)` — SHA-256 hex tiebreaker produces content-ordered, not arrival-ordered, results within the same millisecond

**Category:** Logic Flaws

**Trigger:** Two concurrent writers write children of the same parent snapshot within the same millisecond. Both get the same `created_at_ms`. The `children_of` query sorts by `created_at_ms ASC, id ASC`. The tiebreaker is `id` (SHA-256 hex). SHA-256 hashes of distinct content are uniformly distributed — the order depends on content, not on arrival order. A consumer relying on `children_of` for ordered replay receives children in hash-order within each millisecond bucket.

**Consequence:** Consumers using `children_of` for ordered replay or audit trails get a subtly wrong order for bursts of writes, with no indication that the order is non-deterministic relative to insertion time. The order is deterministic per content but meaningless as a proxy for arrival time.

**Design assumption violated:** The design assumes `created_at_ms ASC, id ASC` produces a stable, arrival-ordered sequence. It does not: the tiebreaker (hash) is correlated with content, not time.

**Suggested mitigation:** Use `config_version ASC` as the tiebreaker instead of `id`. `config_version` is a monotonically increasing integer within an instance by the OCC invariant, directly encoding arrival order. Query: `ORDER BY created_at_ms ASC, config_version ASC`.

---

### [MEDIUM] Offset-based `in_range` pagination with no `offset` cap allows O(K²) index scans

**Category:** Resource Exhaustion

**Trigger:** `in_range(from_ms, to_ms, Page { limit: 1, offset: N })` forces SQLite to scan and discard N rows before returning 1. A caller iterating all snapshots with `limit=1, offset=0`, `limit=1, offset=1`, … `limit=1, offset=K` generates O(K²) total row reads. The design bounds `limit` to 1..=500 but places no maximum on `offset`. A caller can issue `offset = 2^31` legally.

**Consequence:** A single client iterating a large time range drives full index scans on every page request, increasing read latency for all concurrent readers sharing the SQLite WAL. No rate limiting exists in the fetcher.

**Design assumption violated:** The design assumes callers will paginate with reasonable offsets. There is no keyset/cursor pagination alternative and no bound on `offset`.

**Suggested mitigation:** Add a `max_offset: u32` bound (e.g., `offset <= MAX_FETCH_ROWS * 200 = 100,000`) enforced in `Page` validation, returning `FetchError::PageTooLarge` above that threshold. Document that keyset pagination (`WHERE (created_at_ms, id) > (last_ms, last_id)`) is the recommended path for large result sets and should be implemented in Phase 7.

---

### [LOW] `daemon_clock::run_id()` panic fires inside `write()` at step 6, not at `SnapshotWriter::new()`, making uninitialized-clock failures observable only at runtime

**Category:** Logic Flaws

**Trigger:** A caller constructs a `SnapshotWriter` successfully (construction does not call `daemon_clock`), then calls `write()`. If `daemon_clock::init()` was never called (e.g., in an integration test constructed without going through the CLI bootstrap path), the panic fires at step 6 inside the `tokio::time::timeout` future, aborting the task.

**Consequence:** In tests this produces a confusing "task panicked" rather than a structured `WriteError`. In production this is a programmer error caught quickly — but the failure surface is at `write()` call time, not construction time.

**Design assumption violated:** The design assumes `daemon_clock::init()` is always called before `SnapshotWriter::new()`. This invariant is enforced only by convention.

**Suggested mitigation:** Move the `daemon_clock::run_id()` liveness check into `SnapshotWriter::new()`: if `RUN_ID.get().is_none()`, panic immediately at construction time with a clear message. This converts a mid-flight task panic into a predictable startup error.

---

## Summary

**Critical:** 0 &nbsp; **High:** 2 &nbsp; **Medium:** 3 &nbsp; **Low:** 1

**Top concern:** Step-12 `i64` overflow permanently wedges an instance's snapshot chain with no recovery path — every future write is rejected by step 1's negative-version guard, and no mitigation exists short of direct DB surgery. The fix is a single `if current_max == i64::MAX { return Err(WriteError::VersionOverflow) }` guard at step 12.

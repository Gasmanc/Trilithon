# Phase 2 — Aggregate Review Plan

**Generated:** 2026-05-07
**Reviewers:** scope_guardian, security, code_adversarial, qwen, learnings_match, kimi, codex
**Raw findings:** 50 across 7 reviewers
**Unique findings:** 40 after clustering
**Consensus:** 0 unanimous · 0 majority · 40 single-reviewer
**Conflicts:** 0
**Superseded (already fixed):** 0

---

## How to use this document

Feed this file to `/review-remediate` to drive the fix cycle. Each finding has a unique ID (F001, F002, …) that `review-remediate` uses to track state. Do not renumber or delete findings — append `SUPERSEDED` status instead.

---

## CRITICAL Findings

### F001 · [CRITICAL] AuditEventRow missing `prev_hash` field
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/core/src/storage/types.rs` · **Lines:** 134–169
**Description:** `AuditEventRow` is defined without a `prev_hash` field. Slice 2.1 specifies the field `pub prev_hash: String` as part of the canonical struct shape from trait-signatures.md §1 ("`SHA-256 of previous row's canonical JSON (or all-zero for first row); ADR-0009`"). The audit_log DDL also lacks the `prev_hash` column. Without this field, the ADR-0009 hash chain cannot exist in the type system at all.
**Suggestion:** Add `pub prev_hash: String` to `AuditEventRow` in `types.rs`. Add `prev_hash TEXT NOT NULL DEFAULT '0000...0'` column to `audit_log` in `0001_init.sql`.
**Claude's assessment:** AGREE — Phase 2 spec mandates hash chaining for audit log integrity. This is a compliance gap that blocks testing of 2.2 and 2.4.

### F002 · [CRITICAL] `core/crates/core/src/storage/helpers.rs` entirely absent
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/core/src/storage/` · **Lines:** general
**Description:** Slice 2.1 mandates creating `core/crates/core/src/storage/helpers.rs` with the `canonical_json_for_hash()` helper that both `InMemoryStorage` and `SqliteStorage` must call to compute `prev_hash`. The file does not exist. Without it the hash-chain tests required by slices 2.2 and 2.4 cannot compile or run.
**Suggestion:** Create `core/crates/core/src/storage/helpers.rs` with `pub fn canonical_json_for_hash(row: &AuditEventRow) -> String` using `BTreeMap` for deterministic key order.
**Claude's assessment:** AGREE — Architectural mandate, blocking hash chain implementation.

### F003 · [CRITICAL] `proposals` table absent from `0001_init.sql`
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/migrations/0001_init.sql` · **Lines:** 1–122
**Description:** Slice 2.3 requires eight tables: `caddy_instances`, `users`, `sessions`, `snapshots`, `audit_log`, `mutations`, `proposals`, `secrets_metadata`. The diff's `0001_init.sql` contains only seven — the `proposals` table is not present. `enqueue_proposal`, `dequeue_proposal`, and `expire_proposals` on `SqliteStorage` cannot function without this table.
**Suggestion:** Add the `proposals` DDL (with all columns from architecture §6.8, including `wildcard_callout`, `decided_*`, `wildcard_ack_*`, `resulting_mutation`) plus its indexes to `0001_init.sql`.
**Claude's assessment:** AGREE — Phase 2 DDL completeness requirement. Without it, proposal queue cannot exist.

### F004 · [CRITICAL] `schema_migrations` DDL present in migration file (forbidden by spec)
**Consensus:** SINGLE (flagged by 3 reviewers) · scope_guardian, security, code_adversarial
**File:** `core/crates/adapters/migrations/0001_init.sql` · **Lines:** 4–9
**Description:** The TODO states explicitly: "The `_sqlx_migrations` table is created by sqlx at runtime and MUST NOT be in the DDL." Three reviewers flagged this: the diff introduces a `CREATE TABLE schema_migrations` in the migration file which is a custom table sqlx will not recognise, producing an extra table with no writer. This violates the spec and creates schema pollution.
**Suggestion:** Remove the `CREATE TABLE schema_migrations` block from `0001_init.sql`.
**Claude's assessment:** AGREE — Direct spec violation. No custom migration table should exist when sqlx manages `_sqlx_migrations`.

### F005 · [CRITICAL] `snapshots` DDL missing required columns
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/migrations/0001_init.sql` · **Lines:** 48–66
**Description:** Three items mandated by the phase checklist and architecture §6.5 are absent: (1) `created_at_monotonic_ns INTEGER NOT NULL`; (2) `canonical_json_version INTEGER NOT NULL DEFAULT 1`; (3) `CHECK (parent_id != id)` on `parent_id` to prevent self-cycles.
**Suggestion:** Add `created_at_monotonic_ns INTEGER NOT NULL`, `canonical_json_version INTEGER NOT NULL DEFAULT 1`, and `CHECK (parent_id != id)` to the `snapshots` DDL.
**Claude's assessment:** AGREE — Phase 2 schema specification requires all three; missing any one blocks snapshot semantics.

### F006 · [CRITICAL] `audit_log` missing `prev_hash` column in DDL
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/migrations/0001_init.sql` · **Lines:** 68–89
**Description:** The spec requires `prev_hash TEXT NOT NULL DEFAULT '0000...0'` (64 zeros) in `audit_log`. The diff's `audit_log` DDL has no such column. This is the schema-level complement of the missing `prev_hash` in `AuditEventRow`.
**Suggestion:** Add `prev_hash TEXT NOT NULL DEFAULT '0000000000000000000000000000000000000000000000000000000000000000'` to the `audit_log` DDL.
**Claude's assessment:** AGREE — Required for ADR-0009 hash chain; blocking F001.

### F007 · [CRITICAL] SQL-injection-like wildcard injection in `tail_audit_log`
**Consensus:** SINGLE (flagged by 2 reviewers) · qwen, security
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 691–694
**Description:** The `kind_glob` value is sanitised only for a trailing `*` → `%` replacement, but the caller-supplied prefix is passed directly into the LIKE parameter without escaping SQLite LIKE metacharacters `%` and `_`. Any `%` or `_` characters already present in the incoming `kind_glob` value will be treated as LIKE wildcards, broadening the filter beyond the caller's intent and potentially leaking rows from other event kinds. Qwen notes that `"config.applied"` matches `"configXapplied"` in LIKE semantics, and `InMemoryStorage` correctly uses exact-match `String::eq`, creating a divergence where tests pass but production leaks data.
**Suggestion:** Escape `%` and `_` in `kind_glob` before constructing the LIKE parameter. SQLite supports `ESCAPE` in LIKE: replace `%` → `\%` and `_` → `\_` in the prefix portion, then append `%`, and add `ESCAPE '\'` to the static predicate template: `kind LIKE ? ESCAPE '\'`.
**Claude's assessment:** AGREE — Security + correctness gap. The divergence between InMemoryStorage and SqliteStorage implementations is a correctness bug that masked by test-only paths.

---

## HIGH Findings

### F008 · [HIGH] `InMemoryStorage` uses `std::sync::Mutex` instead of `tokio::sync::Mutex`
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/core/src/storage/in_memory.rs` · **Lines:** 19, 38–44
**Description:** Slice 2.2 specifies `use tokio::sync::Mutex`. The diff uses `std::sync::Mutex` throughout. Holding a `std::sync::MutexGuard` across an `.await` point deadlocks the executor; the spec chose `tokio::sync::Mutex` to avoid poisoning and allow safe `.await` inside lock guards.
**Suggestion:** Replace `use std::sync::Mutex` with `use tokio::sync::Mutex` and change all `lock().expect(...)` calls to `.lock().await`.
**Claude's assessment:** AGREE — Specification mandate to prevent deadlock in async context. Critical for test reliability.

### F009 · [HIGH] `PRAGMA application_id` validation and post-migration set step absent
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/src/sqlite_storage.rs`, `core/crates/cli/src/run.rs` · **Lines:** general
**Description:** Slice 2.4 and 2.7 mandate: (a) in `SqliteStorage::open`, reading `PRAGMA application_id` and rejecting values other than `0` or `0x54525754`; (b) in `run.rs` after successful migration, executing `PRAGMA application_id = 0x54525754`. Neither step appears in the diff. The phase checklist requires this.
**Suggestion:** Add the `PRAGMA application_id` read-and-validate in `SqliteStorage::open`. Add the write step in `run.rs` after `apply_migrations` succeeds.
**Claude's assessment:** AGREE — Phase 2 startup integrity gate required by spec.

### F010 · [HIGH] Startup synchronous integrity check not wired
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/cli/src/run.rs` · **Lines:** 75–105
**Description:** Slice 2.6 algorithm and the phase checklist require a synchronous call to `integrity_check_once` before `daemon.started` is emitted. The diff's `run.rs` spawns the periodic task but does not call `integrity_check_once` at startup. `daemon.started` is emitted immediately after migration without the startup integrity gate.
**Suggestion:** After `apply_migrations` and before `daemon.started`, call `integrity_check_once(storage.pool()).await`; if `Failed { detail }`, log and return `ExitCode::StartupPreconditionFailure`.
**Claude's assessment:** AGREE — Architectural requirement for startup safety; blocks 2.6 algorithm.

### F011 · [HIGH] Transaction leaked on spawn-blocking cancellation
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 280–393
**Description:** `insert_snapshot_inner` manually issues `BEGIN IMMEDIATE`, then relies on sequential `ROLLBACK` or `COMMIT` calls to resolve the transaction. The connection is borrowed from the pool (`pool.acquire()`), not from a sqlx `Transaction` guard. If the task that owns this future is cancelled between `BEGIN IMMEDIATE` and the next `ROLLBACK`/`COMMIT` call — which can happen when the `drain_tasks` timeout in `run.rs` fires and `JoinSet::abort_all()` is called — the connection is returned to the pool while the SQLite transaction is still open. The next caller to borrow that connection will inherit an in-progress write transaction, causing `SQLITE_BUSY` or silent data corruption.
**Suggestion:** Replace the raw `BEGIN IMMEDIATE` / `COMMIT` / `ROLLBACK` pattern with sqlx `pool.begin()` / `.begin_immediate()` and let the `Transaction` drop guard issue the rollback automatically; this is resilient to cancellation.
**Claude's assessment:** AGREE — Data integrity hazard on task cancellation. Architectural violation of the three-layer rule.

### F012 · [HIGH] Migration downgrade guard bypassed by version overflow
**Consensus:** SINGLE (flagged by 2 reviewers) · code_adversarial, security
**File:** `core/crates/adapters/src/migrate.rs` · **Lines:** 78, 96
**Description:** The version stored in `_sqlx_migrations` is an `i64` in the database. When it is converted to `u32` via `u32::try_from(v).unwrap_or(0)`, a negative or very large value silently becomes `0`. If a corrupted or attacker-controlled database contains a version of, say, `-1` or `> u32::MAX`, `db_version` is mapped to `0`, which is ≤ `embedded_max`. The downgrade guard passes, and sqlx then attempts to run migrations against a database it should have refused. Code_adversarial notes this is exploitable; security notes it silently saturates to 0.
**Suggestion:** Return `MigrationError::Read` (or a new `MigrationError::VersionOverflow`) instead of silently mapping to 0; use `u32::try_from(v).map_err(|_| MigrationError::Read { source: ... })` and propagate the error.
**Claude's assessment:** AGREE — Downgrade guard is critical for multi-environment consistency. Silent saturation is a logic trap.

### F013 · [HIGH] Storage open/migrate errors exit with wrong code (1 not 3)
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/cli/src/run.rs` · **Lines:** 77–91
**Description:** Both `SqliteStorage::open` and `apply_migrations` errors are converted to `anyhow::Error` via `map_err(|e| anyhow::anyhow!(...))` and returned with `?`. The error is erased into `anyhow::Error` first, so if a caller ever constructs the error path differently (e.g. in a future function that returns `MigrationError` directly), the two conversion paths diverge. More immediately: the `LockError::AlreadyHeld` case in `lock.rs` is wrapped via `std::io::Error::other(e.to_string())`, which discards the structured error type; monitoring code that inspects `StorageError` variants cannot distinguish "lock held" from any other I/O failure. Code_adversarial suggests adding a `StorageError::LockHeld` variant.
**Suggestion:** Add a `StorageError::LockHeld` variant so that the lock-held condition is not erased into a generic `Io` error. In `run.rs`, map `StorageError::LockHeld` to `ExitCode::AlreadyRunning`.
**Claude's assessment:** AGREE — Operational visibility requirement. Operators need to distinguish lock conflicts from other startup failures.

### F014 · [HIGH] Tail audit log performance no offset/limit pagination
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 673–782
**Description:** The `tail_audit_log` query has no index hint optimization for the `ORDER BY occurred_at DESC` clause when no `since`/`until` filter is present. With the existing `audit_log_occurred_at` index, SQLite can perform an index-only reverse scan, but the query as written may trigger a filesort for wide range queries. More critically, there is no pagination mechanism — callers requesting large limits will materialize the entire result set in memory via `fetch_all`. For an audit log that grows unbounded, this will degrade over time.
**Suggestion:** Use cursor-based pagination (pass the last seen `occurred_at` and id) rather than limit-only queries, or at minimum document that `limit` should be bounded by callers.
**Claude's assessment:** AGREE — Scalability issue. Unbounded `fetch_all` on a growing audit log is a footgun.

### F015 · [HIGH] Missing data directory check produces unclear error
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 65–69
**Description:** When `data_dir` does not exist, `LockHandle::acquire` fails with a generic I/O error like "No such file or directory". The operator sees a confusing message about acquiring a lock rather than "data directory does not exist". The CLI integration test `missing_data_dir_exits_3` accepts exit code 2 OR 3, indicating the error path is not deterministic (config loader may intercept first).
**Suggestion:** Check `data_dir.exists()` and return a clear `StorageError::Io` with a descriptive message, or validate in the config loader before reaching `SqliteStorage::open`.
**Claude's assessment:** AGREE — Operational clarity. Operators need direct feedback about missing directories.

### F016 · [HIGH] Stderr ready signal can block and hang test
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/cli/tests/utc_timestamps.rs` · **Lines:** 70–79
**Description:** The stderr reader thread uses `mpsc::sync_channel(1)` and calls `ready_tx.send(())` for every JSON line. After the main thread performs a single `recv_timeout`, further sends can block once the one-slot buffer fills, which stops stderr draining and can deadlock/hang the test (and potentially the child process) under moderate log volume.
**Suggestion:** Signal readiness only once with a non-blocking path (for example, guard with a `sent_ready` flag and skip later sends), or use `try_send` and ignore `Full`, or switch to an unbounded `mpsc::channel()` for the readiness signal.
**Claude's assessment:** AGREE — Test infrastructure bug that can cause intermittent hangs.

### F017 · [HIGH] Migration applied_count calculation error
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/migrate.rs` · **Lines:** 117–118
**Description:** `applied_count` is computed as `current_version - db_version` using version numbers, not row counts. If migration versions are non-sequential (e.g., 1, 2, 5), then `applied_count = 5 - 2 = 3` is correct by coincidence, but with gaps like versions 1, 10, it would report `9` applied when only 1 migration ran. sqlx does not mandate sequential versions. The value is used in the `tracing::info!` log line and exposed in `MigrationOutcome`, which downstream consumers may rely on for audit trails.
**Suggestion:** Query `SELECT COUNT(*) FROM _sqlx_migrations WHERE version > ?` to count actual rows applied, or document that versions must be sequential and enforce it at build time.
**Claude's assessment:** AGREE — Audit trail correctness. Downstream consumers rely on accurate applied_count.

---

## WARNING Findings

### F018 · [WARNING] Unparameterised database URL construction from path display
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 72
**Description:** The SQLite connection URL is constructed by embedding `data_dir.display()` directly via `format!` into the URL string: `format!("sqlite://{}/trilithon.db", data_dir.display())`. On a path containing spaces, percent-signs, or characters that are meaningful in a URL (e.g. a `#` in a directory name), `display()` does not URL-encode the path, producing a syntactically invalid sqlite URL. `SqliteConnectOptions` provides a `filename()` builder method that accepts a `Path` directly and handles encoding internally.
**Suggestion:** Replace `format!("sqlite://{}/trilithon.db", ...)` + `from_str` with `SqliteConnectOptions::new().filename(data_dir.join("trilithon.db"))`, which takes a `Path` and requires no string encoding.
**Claude's assessment:** AGREE — Operational robustness. Special characters in datadir paths are common in deployment.

### F019 · [WARNING] Integrity-check error detail exposed to structured logs
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/src/integrity_check.rs` · **Lines:** 58
**Description:** On an integrity failure, the raw SQLite `PRAGMA integrity_check` output is emitted verbatim as a structured `tracing::error!` field: `tracing::error!(detail = %detail, "storage.integrity_check.failed")`. PRAGMA output can include internal page numbers, row-IDs, B-tree node addresses, and partial row content. If the logging backend forwards structured events to a log aggregator accessible beyond the operator, this leaks internal storage structure and potentially partial row data.
**Suggestion:** Replace `%detail` with a fixed marker or a byte-length count. If the full detail is needed for forensics, log it only at `tracing::debug!` or write it to a local-only file.
**Claude's assessment:** AGREE — Information disclosure risk. PRAGMA integrity_check output is not suitable for external log aggregation.

### F020 · [WARNING] Integrity check uses `fetch_one`, loses multi-row results
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/adapters/src/integrity_check.rs` · **Lines:** 30–32
**Description:** `PRAGMA integrity_check` returns one or more rows — one row with `"ok"` when healthy, or multiple rows describing individual problems. `fetch_one` returns an error if the result set is empty. More importantly, when the database reports multiple problems it returns only the first row and silently discards the rest. The `IntegrityResult::Failed { detail }` variant therefore only surfaces one line of a multi-line corruption report.
**Suggestion:** Use `fetch_all` and join the rows with newlines into `detail`, or document the behavior in the function's doc comment.
**Claude's assessment:** AGREE — Diagnostics completeness. Multi-problem reports must be fully reported.

### F021 · [WARNING] Shutdown observer signal race in integrity loop
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/integrity_check.rs` · **Lines:** 52–68
**Description:** In the `tokio::select!` loop, the `ticker.tick()` and `shutdown.changed()` branches have equal priority. If both are ready in the same poll iteration, `tokio::select!` picks randomly. This means a shutdown signal could be delayed by one full interval (6 hours by default) if the ticker wins the race. More concerning: if the ticker fires and the integrity check itself takes a long time (corrupt DB), shutdown will wait for it to complete before checking the signal again.
**Suggestion:** Add a timeout around `integrity_check_once` (e.g., 30 seconds) to bound the worst-case shutdown latency, or use `tokio::select!` with biased polling towards the shutdown branch.
**Claude's assessment:** AGREE — Operational safety. Unbounded shutdown latency due to long-running checks is a problem.

### F022 · [WARNING] Insert snapshot redundant hash computation
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 525–530
**Description:** `insert_snapshot` calls `validate_snapshot_invariants` which computes `content_address_bytes(desired_state_json)` (SHA-256). Then `insert_snapshot_inner` performs multiple database queries. The SHA-256 computation is ~O(n) where n is the JSON payload size. For large configs, this adds measurable latency before the transaction even starts. While the validation-before-transaction ordering is correct (fail fast), the hash is computed unconditionally even for idempotent duplicates that could be detected by a quick DB lookup first.
**Suggestion:** Consider checking for existing ID first (fast index lookup) before computing SHA-256, to short-circuit idempotent re-inserts.
**Claude's assessment:** AGREE — Performance optimization opportunity for common path.

### F023 · [WARNING] Daemon CLI no longer exposes `--version`
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/cli/src/cli.rs`, `core/crates/cli/src/main.rs` · **Lines:** general
**Description:** `cli.rs` removes `version` from the `#[command(...)]` derive and adds `disable_version_flag = true`. `main.rs` deletes the `ErrorKind::DisplayVersion` arm, so any future attempt to invoke `--version` falls through to the usage-error path and exits 64. No custom `--version` argument or subcommand replaces it, yet `build.rs` still compiles in `TRILITHON_GIT_SHORT_HASH`.
**Suggestion:** Re-enable `#[command(version)]` so the standard `-V` / `--version` flag works, or add a dedicated `--version` argument that prints the version plus git hash.
**Claude's assessment:** AGREE — Operational usability. Operators expect `--version` on all CLIs.

### F024 · [WARNING] Spec-required shutdown APIs removed despite planned later use
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/cli/src/shutdown.rs` · **Lines:** general
**Description:** `ShutdownSignal::is_shutting_down()` and `ShutdownController::signal()` are deleted. Both functions were explicitly annotated `#[expect(dead_code, reason = "spec-required API, callers added in later slices")]`, indicating they were reserved by design for future slices. Phase 2.7 involves wiring startup and integration tests that may need to poll shutdown state or hand signals to late-spawned tasks.
**Suggestion:** Retain these two APIs if the spec still requires them; otherwise update the architecture docs to reflect that the shutdown API surface has been deliberately narrowed.
**Claude's assessment:** DISAGREE on deletion — The `#[expect(...)]` annotations explicitly state these APIs are reserved by spec for later slices. Removing them suggests the spec was not consulted. Recommend restoring them to `shutdown.rs`.

### F025 · [WARNING] Phase 2 review documents added to repo root
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `docs/phase-02-review.md`, `docs/phase-02-unfixed.md` · **Lines:** general
**Description:** These files are not referenced by any TODO work unit or the phase exit checklist. These appear to be reviewer artefacts (in-flight review outputs) that belong under `docs/In_Flight_Reviews/` or `docs/End_of_Phase_Reviews/`, not at the repo root.
**Suggestion:** Move these files to the established review artefacts directories or exclude them from the phase commit.
**Claude's assessment:** AGREE — Repo hygiene. Temporary review files should not be committed at root.

### F026 · [WARNING] ShutdownObserver trait method renamed from spec
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/core/src/lifecycle.rs` · **Lines:** 10–19
**Description:** The TODO specifies `async fn wait(&mut self)` as the single method on `ShutdownObserver`. The diff defines `fn changed(&mut self) -> Pin<Box<dyn Future<...>>>` instead, and adds a second method `fn is_shutting_down(&self) -> bool`. The rename and the extra method are not mandated.
**Suggestion:** Either align the trait with the spec (`async fn wait(&mut self)` only) or document the divergence as an explicit architectural decision.
**Claude's assessment:** AGREE on alignment — The rename from `wait` to `changed` changes API semantics without annotation. Recommend either restoring spec-name or adding a comment justifying the divergence.

### F027 · [WARNING] Redaction sites overflow silently clamps to 0 on read
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 774
**Description:** `u32::try_from(redaction_sites_raw).unwrap_or(0)` silently replaces a negative or out-of-range `i64` stored in the database with `0`. The `redaction_sites` field counts how many secret fields were redacted from `redacted_diff_json`. If a corrupted or adversarially written row stores a negative value, the returned `AuditEventRow` will claim zero redactions occurred for a diff that may in fact contain un-redacted secrets.
**Suggestion:** Return a `StorageError::Integrity` when `redaction_sites_raw < 0` rather than substituting 0, so callers cannot silently treat a tampered row as fully redacted.
**Claude's assessment:** AGREE — Security issue. Negative redaction counts must not be silently normalized to zero.

### F028 · [WARNING] Known pattern: sqlite-begin-immediate-read-check-write
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general
**Lines:** general
**Description:** Pattern from `docs/solutions/runtime-errors/sqlite-begin-immediate-read-check-write-2026-05-05.md` — SQLite read-check-write sequences must use BEGIN IMMEDIATE (not DEFERRED) to acquire the write lock before the invariant check, preventing another writer from inserting between the read and the INSERT.
**Suggestion:** Review `docs/solutions/runtime-errors/sqlite-begin-immediate-read-check-write-2026-05-05.md` before implementing snapshot/proposal insertion.
**Claude's assessment:** AGREE — Learnings-researcher surfaced a known solution pattern. Likely applies to F011 (transaction leak).

### F029 · [WARNING] Known pattern: migration-bootstrap-no-such-table-2026-05-03
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general
**Lines:** general
**Description:** Pattern from `docs/solutions/runtime-errors/migration-bootstrap-no-such-table-2026-05-03.md` — When reading migration state from a table that may not exist yet, match on the "no such table" error message to return version 0 for a fresh DB, and propagate everything else.
**Suggestion:** Review `docs/solutions/runtime-errors/migration-bootstrap-no-such-table-2026-05-03.md` before finalizing migration startup logic.
**Claude's assessment:** AGREE — Applies to fresh database initialization path in migration runner.

### F030 · [WARNING] Known pattern: sqlite-manual-tx-rollback-early-exit
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general
**Lines:** general
**Description:** Pattern from `docs/solutions/runtime-errors/sqlite-manual-tx-rollback-early-exit-2026-05-05.md` — When managing SQLite transactions with raw SQL (BEGIN IMMEDIATE / COMMIT), every early-exit code path must issue an explicit ROLLBACK, or the write lock is held until the connection drops.
**Suggestion:** Review `docs/solutions/runtime-errors/sqlite-manual-tx-rollback-early-exit-2026-05-05.md` — this directly applies to F011 (transaction leak).
**Claude's assessment:** AGREE — Core issue behind F011. Manual transaction management requires explicit cleanup on all exit paths.

### F031 · [WARNING] Known pattern: sqlite-extended-error-codes-mask-2026-05-03
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general
**Lines:** general
**Description:** Pattern from `docs/solutions/runtime-errors/sqlite-extended-error-codes-mask-2026-05-03.md` — Always match SQLite error codes on `code & 0xFF` — extended codes include the base code in their low 8 bits and won't match a bare code number.
**Suggestion:** Review `docs/solutions/runtime-errors/sqlite-extended-error-codes-mask-2026-05-03.md` when implementing error handling in migrate.rs and sqlite_storage.rs.
**Claude's assessment:** AGREE — Common pitfall in SQLite error handling.

### F032 · [WARNING] Known pattern: storage-trait-error-variant-parity-2026-05-05
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general
**Lines:** general
**Description:** Pattern from `docs/solutions/runtime-errors/storage-trait-error-variant-parity-2026-05-05.md` — When a Storage trait has multiple implementations, every impl must return the same error variant for the same failure condition — divergence makes tests pass while production silently breaks.
**Suggestion:** Review `docs/solutions/runtime-errors/storage-trait-error-variant-parity-2026-05-05.md` before finalizing InMemoryStorage and SqliteStorage implementations.
**Claude's assessment:** AGREE — Applies to F007 (LIKE injection divergence) and general trait/impl consistency.

---

## SUGGESTION / LOW Findings

### F033 · [SUGGESTION] Lock handle drop ignores unlock error
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/lock.rs` · **Lines:** 59–64
**Description:** The `Drop` implementation silently ignores unlock failures (`let _ = FileExt::unlock(&self.file)`). If the unlock fails (e.g., fd closed prematurely, filesystem error), the lock file persists on disk. This is benign for the advisory lock semantics (the lock is process-scoped and released on fd close anyway), but could confuse operators inspecting leftover lock files.
**Suggestion:** Add a `tracing::warn!` on unlock failure for observability, or document the behavior explicitly.
**Claude's assessment:** AGREE — Operational debugging aid. Silent unlock failures should be logged.

### F034 · [SUGGESTION] Database error context masking via sanitization
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/db_errors.rs` · **Lines:** 12–36
**Description:** The `sqlx_err` function maps all non-specific database errors to `SqliteErrorKind::Other(e.to_string())`, which includes the full sqlx error message. These error strings often contain internal details (connection URLs, file paths) that leak into logs. Since `StorageError::Sqlite { kind }` displays as `"sqlite error: Other(\"...\")"`, the full string appears in operator-visible output.
**Suggestion:** Either sanitize the error string before storing in `Other`, or add a separate variant for "disk IO" errors that commonly occur in SQLite and don't need the full context string.
**Claude's assessment:** AGREE — Information disclosure risk. Error messages should not contain filesystem paths or connection URLs.

### F035 · [SUGGESTION] Tail audit log uses dynamic SQL format string
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 713–730
**Description:** The WHERE clause is assembled via `format!()` using a vector of `'static str` literals. While the author correctly notes that all user input goes through bind parameters (safe from injection), the dynamic format-string approach is harder to audit than a static query with optional subqueries. Any future addition to the condition-building logic could inadvertently introduce user-controlled SQL text.
**Suggestion:** Consider using a query builder library or at minimum add a compile-time assertion that `conditions` only contains static strings, to make the security invariant self-documenting.
**Claude's assessment:** AGREE — Maintainability. Future editors might not understand why this pattern is safe.

### F036 · [SUGGESTION] Row-to-snapshot error propagation verbosity
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 448–484
**Description:** Every field extraction uses `.map_err(sqlx_err)?` individually. If `try_get` fails partway through (e.g., column type mismatch), the function returns early with a `StorageError` but the partially-constructed data is lost. This is correct behavior (never return partial rows), but the error messages from `sqlx::Error::ColumnIndex` are cryptic (e.g., "column index 7 not found").
**Suggestion:** Map column errors to a more descriptive `StorageError::Integrity` with the column name included.
**Claude's assessment:** AGREE — Diagnostics clarity. Column errors should include the column name.

### F037 · [SUGGESTION] Trailing dot in environment key produces cryptic error
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** ~277
**Description:** The empty-segment guard catches `..` and leading dots, but a trailing dot such as `TRILITHON_SERVER_BIND_` (which becomes `server.bind.`) is only caught one recursion level later when `dotted_key` is `""`. The resulting error says `key contains an empty segment: ""` instead of the original key.
**Suggestion:** Add `dotted_key.ends_with('.')` to the guard so the error message preserves the full offending key.
**Claude's assessment:** AGREE — Error message clarity.

### F038 · [SUGGESTION] Non-Unicode environment variables silently ignored
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/adapters/src/env_provider.rs` · **Lines:** ~16
**Description:** The new comment correctly notes that `vars()` silently skips non-Unicode entries, so a non-UTF-8 `TRILITHON_*` variable is dropped without feedback. A user who sets such a variable may never realise the override is being ignored.
**Suggestion:** During config loading, scan `std::env::vars_os()` for `TRILITHON_*` keys that fail UTF-8 validation and emit a `tracing::warn!` so the omission is visible.
**Claude's assessment:** AGREE — Operational visibility.

### F039 · [SUGGESTION] User-facing log-format warning uses debug quoting
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/cli/src/observability.rs` · **Lines:** ~125
**Description:** The unknown-format warning writes `TRILITHON_LOG_FORMAT={v:?}`, which renders the value with quotes and escapes (e.g., `"json"` instead of `json`).
**Suggestion:** Use `{v}` instead of `{v:?}` in the `writeln!` call so the warning reads naturally.
**Claude's assessment:** AGREE — UX polish.

### F040 · [SUGGESTION] Advisory lock survives across fork without FD_CLOEXEC
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/adapters/src/lock.rs` · **Lines:** 35–50
**Description:** The `File` created for the lock is opened without setting `O_CLOEXEC` / `FD_CLOEXEC`. On a `fork`+`exec` (e.g. if the daemon ever spawns a child process), the child process inherits the open file descriptor and the OS-level lock. The child will hold the lock without owning a `LockHandle`, so the parent's `LockHandle::drop` will release the lock while the child still holds the file descriptor.
**Suggestion:** After opening the file, call `nix::fcntl::fcntl(fd, F_SETFD, FD_CLOEXEC)` (or use `std::os::unix::fs::OpenOptionsExt::custom_flags(libc::O_CLOEXEC)` on the `OpenOptions`) to prevent the descriptor from being inherited across exec.
**Claude's assessment:** AGREE — Uncommon but important if daemon ever spawns child processes. Safe to defer to Phase 3 unless spec requires subprocess spawning.

---

## CONFLICTS (require human decision before fixing)

**None detected.** All 40 findings resolved through clustering and consensus marking. No reviewers gave contradictory suggestions for the same issue.

---

## Out-of-scope / Superseded

None. All findings are actionable for Phase 2 remediation.

---

## Summary statistics

| Severity | Unanimous | Majority | Single | Total |
|----------|-----------|----------|--------|-------|
| CRITICAL | 0 | 0 | 7 | 7 |
| HIGH | 0 | 0 | 10 | 10 |
| WARNING | 0 | 0 | 13 | 13 |
| SUGGESTION | 0 | 0 | 10 | 10 |
| **Total** | **0** | **0** | **40** | **40** |

---

## Assessment

**Phase 2 Completeness:** Phase 2 implementation has **structural gaps** that block all downstream work:

1. **Schema defects (F001–F006, F007):** Missing `prev_hash` fields, missing `helpers.rs` module, missing `proposals` table, forbidden `schema_migrations` table, incomplete `snapshots` DDL. These are blockers for hash-chaining and proposal-queue tests.

2. **Startup safety (F009–F010, F012–F013):** Missing PRAGMA validation, missing startup integrity check, migration version overflow bypass. These are pre-daemon-startup gates required by Phase 2.7.

3. **Data integrity (F011, F018–F020, F027):** Transaction leak on cancellation, DB URL encoding, fetch_one loss, redaction overflow. These affect data consistency under edge cases.

4. **Specification divergence (F024, F026):** Shutdown APIs and trait methods removed contrary to spec. These are intentional departures that should be documented.

5. **Learnings alignment (F028–F032):** Known solution patterns from Phase 1 remediation apply directly to current implementation gaps.

**Recommendation:** Address all CRITICAL findings (F001–F007) first, then HIGH findings (F008–F017), then WARNING findings. F040 can be deferred if subprocess spawning is not in Phase 2 scope.

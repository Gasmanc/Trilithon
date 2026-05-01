# ADR-0006: Use SQLite as the V1 persistence layer behind a Storage adapter trait

## Status

Accepted — 2026-04-30.

## Context

Trilithon owns the desired-state model, the snapshot history (T1.2),
the audit log (T1.7), the secrets vault metadata (T1.15), session
records (T1.14), and the proposal queue (T2.1, T2.4). All of this must
be persisted across daemon restarts, must survive power loss, and must
not lose the audit log even under filesystem misbehaviour (hazard H14).

The binding prompt (section 2, item 6) fixes the V1 choice: SQLite for
single-instance V1, with PostgreSQL as a V2+ option behind a `Storage`
adapter trait. PostgreSQL is not a V1 deliverable.

Forces:

1. **Single-instance V1.** Trilithon V1 manages one Caddy instance per
   deployment. Multi-instance fleet management is T3.1 and out of
   scope. A single-process embedded database is sufficient and
   removes an entire operational concern (running and securing a
   separate database server).
2. **Local-first.** The user's data lives on the user's hardware
   (constraint 14). SQLite stores its data in a single file the user
   can back up, copy, or destroy with standard tools. PostgreSQL
   would require a separate service, a separate authentication
   surface, and a separate backup pipeline.
3. **Hazard H14: corruption resilience.** SQLite's Write-Ahead Log
   (WAL) mode plus periodic `PRAGMA integrity_check` provide a
   well-understood recovery posture. The audit log MUST NOT be lost
   on power failure.
4. **Concurrency.** Trilithon's mutation pipeline is single-writer
   by design (T2.10 optimistic concurrency on `config_version`).
   SQLite's writer-serialisation model fits this naturally;
   PostgreSQL's MVCC offers nothing the workload needs at V1 scale.
5. **Forward path.** ADR-0003 places the database adapter in
   `crates/adapters` behind a `Storage` trait declared in
   `crates/core`. T3.1 (multi-instance fleet management) may justify
   PostgreSQL later. The trait must be shaped so that swapping the
   implementation is a tractable engineering task, not a rewrite.

## Decision

Trilithon's V1 persistence layer SHALL be SQLite, version 3.40 or
later. The database SHALL run in WAL journal mode (`PRAGMA
journal_mode = WAL;`) with `PRAGMA synchronous = NORMAL;` and `PRAGMA
foreign_keys = ON;`. The daemon SHALL run `PRAGMA integrity_check;`
on startup and SHALL log the result through the tracing subscriber
(hazard H14).

A `Storage` trait SHALL be declared in `crates/core`. The trait SHALL
expose operations in domain terms (insert snapshot, fetch snapshot by
identifier, list snapshots since cursor, append audit record, list
audit records by correlation identifier, persist secret blob, fetch
secret blob, increment and check `config_version` for optimistic
concurrency, and so on). The trait SHALL NOT expose SQL strings, row
types, or transaction objects.

The SQLite implementation of `Storage` SHALL live in `crates/adapters`
and SHALL be the only implementation in V1. The `Storage` trait SHALL
NOT be widened with SQLite-specific operations.

The schema SHALL include a `caddy_instance_id` column on every
configuration object, hard-coded to `local` in V1, reserved for T3.1
multi-instance use per the binding prompt.

Snapshots, audit records, and secrets-vault entries SHALL be insert-only.
There SHALL NOT be `UPDATE` or `DELETE` statements against the
`snapshots` or `audit_log` tables anywhere in the codebase (T1.2,
T1.7, hazards H10, H14). Schema migrations SHALL be additive: new
columns, new tables, new indexes, never destructive rewrites of
audit history.

Backup (T2.12) SHALL be implemented through SQLite's `VACUUM INTO`
or the online backup API, not through `cp` of the live database file.

PostgreSQL SHALL NOT appear in V1 dependencies, V1 documentation
(except as a Tier 3 reference), or V1 code paths. A PostgreSQL
implementation of `Storage` MAY land in V2 behind its own ADR.

## Consequences

**Positive.**

- One file holds the user's state. Backup, restore, and disaster
  recovery are tractable for non-experts.
- The `Storage` trait isolates the rest of Trilithon from SQL.
  Tests in `core` exercise the mutation pipeline against an
  in-memory implementation of `Storage`; production exercises the
  SQLite implementation. The constraint against mocks in
  production paths (constraint 8) is honoured because the
  in-memory implementation is real, not a mock.
- WAL mode delivers concurrent reads alongside the single writer,
  which serves the access-log viewer (T2.5) and drift detection
  (T1.4) without contention against the mutation pipeline.

**Negative.**

- SQLite's writer-serialisation model means a mutation in flight
  blocks another mutation. T2.10's optimistic concurrency turns
  this into a typed conflict, but the friction is real for users
  who expect database-level concurrency.
- A single SQLite file is a single point of failure. Backup
  (T2.12) is non-optional. Users who do not back up will lose
  state on disk failure.
- Multi-instance fleet management (T3.1) will eventually require
  either schema redesign or migration to PostgreSQL. The
  `Storage` trait positions Trilithon for the latter at the cost
  of foreclosing the former.

**Neutral.**

- The Rust SQLite binding choice is open: `rusqlite` (synchronous,
  bundled SQLite) or `sqlx` (async, derives types from queries) are
  both viable. The choice is recorded in the architecture document,
  not this ADR.
- File-system permission on the SQLite file SHALL be `0600` and the
  daemon SHALL be the owner. This is a deployment concern but
  worth recording.

## Alternatives considered

**PostgreSQL from V1.** Run PostgreSQL alongside the daemon. Rejected
because the binding prompt forbids it for V1 (constraint 6), because
single-instance V1 does not need MVCC or network database access, and
because shipping PostgreSQL doubles the operational surface for the
local-first deployment target.

**File-system-only persistence (JSON files in a directory).** Store
each snapshot as a file under a content-addressed path. Rejected
because the audit log requires queries (range by correlation
identifier, range by time, range by actor) that filesystem walks
cannot serve at the scale V1 will produce, and because filesystem
crash semantics across operating systems are weaker than SQLite's
WAL-mode guarantees.

**An embedded key-value store (sled, RocksDB).** Use a key-value
store and build query indexes by hand. Rejected because the audit
log and snapshot history want relational queries, and because
SQLite's schema enforcement and `integrity_check` posture address
hazard H14 more directly than an LSM-based key-value store.

**libsql or DuckDB.** libsql is a SQLite fork with a server mode;
DuckDB is column-oriented. Rejected because libsql's server mode is
unnecessary for V1 and because DuckDB's strengths (analytical
queries) match a Tier 3 access-log workload (T3.10), not the V1
control-plane workload.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#2-non-negotiable-constraints`,
  item 6; section 4 features T1.1, T1.2, T1.7, T1.14, T1.15;
  section 5 features T2.1, T2.4, T2.5, T2.10, T2.12; section 6
  feature T3.1; section 7 hazards H10, H14.
- ADR-0003 (Rust three-layer workspace architecture).
- ADR-0009 (Immutable content-addressed snapshots and audit log).
- ADR-0014 (Secrets encrypted at rest with keychain master key).
- SQLite documentation: "Write-Ahead Logging" and "Backup API,"
  SQLite 3.40.

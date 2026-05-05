---
track: bug
problem_type: race-condition
root_cause: concurrency-gap
resolution_type: lock-added
severity: high
title: "Use BEGIN IMMEDIATE for SQLite read-check-write sequences to prevent TOCTOU races"
slug: sqlite-begin-immediate-read-check-write
date: 2026-05-05
phase_id: "5"
generalizable: true
one_sentence_lesson: "SQLite read-check-write sequences must use BEGIN IMMEDIATE (not DEFERRED) to acquire the write lock before the invariant check, preventing another writer from inserting between the read and the INSERT."
tags: [rust, sqlite, sqlx, concurrency, toctou, transactions]
---

## Context

The `insert_snapshot` path in `SqliteStorage` checks several invariants before committing: parent exists, monotonicity holds, no hash collision. The initial implementation opened a DEFERRED transaction via `pool.begin()`, which defers write-lock acquisition until the first actual write.

## What Happened

A DEFERRED transaction holds only a shared read lock until it issues a write. Between the invariant-check SELECTs and the INSERT, another concurrent writer can commit a row that violates the invariants the first writer already checked. In a daemon process that processes proposals concurrently, this creates a window where two writers both pass the monotonicity check and both insert, producing duplicate or out-of-order `config_version` values.

The fix: acquire a connection from the pool directly (`pool.acquire().await?`) and issue `EXECUTE "BEGIN IMMEDIATE"` before any reads. IMMEDIATE acquires a write (reserved) lock upfront, blocking other writers for the duration of the check-then-insert sequence.

```rust
// Before (DEFERRED — TOCTOU window):
let mut tx = pool.begin().await?;

// After (IMMEDIATE — write lock acquired before any reads):
let mut conn = pool.acquire().await?;
sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
// ... invariant checks ...
// COMMIT or explicit ROLLBACK
```

## Lesson

> SQLite read-check-write sequences must use BEGIN IMMEDIATE (not DEFERRED) to acquire the write lock before the invariant check, preventing another writer from inserting between the read and the INSERT.

## Applies When

- Any write path that reads one or more rows to check invariants before inserting
- Monotonicity, uniqueness, or parent-existence checks in SQLite
- sqlx connection pool usage where `pool.begin()` would produce a DEFERRED transaction

## Does Not Apply When

- The operation is a pure INSERT with no prior invariant check (DEFERRED is fine)
- The table is write-serialised by a higher-level application lock that already excludes concurrent writers

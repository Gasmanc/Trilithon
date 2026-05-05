---
track: bug
problem_type: race-condition
root_cause: missing-error-handling
resolution_type: guard-added
severity: high
title: "Explicitly ROLLBACK on every early exit when managing SQLite transactions with raw SQL"
slug: sqlite-manual-tx-rollback-early-exit
date: 2026-05-05
phase_id: "5"
generalizable: true
one_sentence_lesson: "When managing SQLite transactions with raw SQL (BEGIN IMMEDIATE / COMMIT), every early-exit code path — including invariant failures and duplicate detection — must issue an explicit ROLLBACK, or the write lock is held until the connection drops."
tags: [rust, sqlite, sqlx, transactions, resource-leak, deadlock]
---

## Context

`insert_snapshot_inner` used `BEGIN IMMEDIATE` via raw SQL instead of `pool.begin()` (to acquire the write lock before invariant checks). The initial implementation had several early-return branches (hash collision, intent too long, parent not found) that returned `Err(...)` directly without issuing ROLLBACK.

## What Happened

SQLite IMMEDIATE transactions that are neither committed nor rolled back hold the write lock (a reserved lock) until the connection is returned to the pool. With sqlx's connection pool, a connection is returned when the `PoolConnection` guard is dropped. In async Rust, the drop order is not always obvious — in practice the connection is held for the remainder of the enclosing async block, meaning every early-return path extended the lock window unnecessarily. In a busy daemon this degrades throughput; in a daemon with a small pool it causes latency spikes.

The fix: issue `sqlx::query("ROLLBACK").execute(&mut *conn).await?` before every early `return Err(...)`.

```rust
// Every early exit must explicitly rollback:
if invariant_fails {
    sqlx::query("ROLLBACK").execute(&mut *conn).await?;
    return Err(StorageError::...);
}
```

## Lesson

> When managing SQLite transactions with raw SQL (BEGIN IMMEDIATE / COMMIT), every early-exit code path — including invariant failures and duplicate detection — must issue an explicit ROLLBACK, or the write lock is held until the connection drops.

## Applies When

- Any write path that opens a transaction with raw `BEGIN` / `BEGIN IMMEDIATE` SQL instead of the ORM's transaction API
- sqlx code that uses `pool.acquire()` + manual SQL instead of `pool.begin()` (the latter auto-rollbacks on drop)
- Functions with multiple early-return branches after a BEGIN

## Does Not Apply When

- Using sqlx `pool.begin()` — the returned `Transaction` guard automatically issues ROLLBACK on drop
- The connection is closed immediately after the early return, making the lock window negligible

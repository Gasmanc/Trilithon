---
name: Distinguish "no such table" from real DB errors in migration bootstrap
description: Reading migration state before the migrations table exists must distinguish the "table missing" case (fresh DB) from real I/O errors — treating both as version 0 silently swallows corruption
type: solution
category: runtime-errors
phase_id: onboard-git-history
source_commit: f4b15d1
source_date: 2026-05-02
one_sentence_lesson: When reading migration state from a table that may not exist yet, match on the "no such table" error message to return version 0 for a fresh DB, and propagate everything else — otherwise real DB errors are silently treated as a clean slate.
---

## Problem

`apply_migrations` read the current schema version with a query that could fail in two fundamentally different ways:

```rust
let db_version = match sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
    .fetch_one(pool)
    .await
{
    Ok(Some(v)) => v,
    Ok(None) | Err(_) => 0,   // swallows I/O errors, corruption, etc.
};
```

A real DB error (permission denied, corrupt page) was silently treated as "fresh database, run all migrations", potentially destroying data.

## Fix

Narrow the error match to the specific case that means "fresh DB":

```rust
let db_version = match sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
    .fetch_one(pool)
    .await
{
    Ok(Some(v)) => u32::try_from(v).unwrap_or(0),
    Ok(None) => 0,
    Err(sqlx::Error::Database(ref db_err))
        if db_err.message().contains("no such table") =>
    {
        0
    }
    Err(e) => return Err(MigrationError::Read { source: e }),
};
```

## When to apply

Any migration or schema-bootstrap code that reads state from a table that may not exist on first run. The same pattern applies in non-Rust ORMs: catch the specific "table not found" error, propagate everything else.

## Category

runtime-errors

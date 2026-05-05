---
name: Don't run migrations inside open() — caller controls migration timing
description: Mixing apply_migrations() into open() prevents callers from controlling the order; seed migrations and other setup steps that depend on correct timing silently break
type: solution
category: architecture-patterns
phase_id: onboard-git-history
source_commit: f4b15d1
source_date: 2026-05-02
one_sentence_lesson: Keep open() and apply_migrations() separate — callers that need to seed FK parents before probing the DB cannot do so if migrations run inside open() before they get control.
---

## Problem

`SqliteStorage::open` called `sqlx::migrate!()` directly. This meant:
- Callers couldn't control when migrations ran relative to seeding
- A seed migration added later (0003_seed_local_instance) had to be added to the migration set, but `run.rs` already called migrations separately — the internal call inside `open()` ran first with no seeding, causing FK failures

## Fix

`open()` only opens and validates the connection. The caller (`run.rs`) calls `apply_migrations()` explicitly at the right point in the startup sequence:

```rust
// run.rs
let pool = SqliteStorage::open(&db_path).await?;
apply_migrations(&pool).await?;           // caller controls timing
seed_local_instance(&pool).await?;        // can now run after migrations
let probe_store = CapabilityStore::new(pool.clone());
```

## When to apply

Any time you have a database connection wrapper. The `open()` function should establish the connection and optionally run a quick `PRAGMA` check, but never apply schema changes. Separate the migration step so callers can interleave seeding and validation.

## Category

architecture-patterns

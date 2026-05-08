---
track: knowledge
problem_type: best-practice
title: "Detect SQLite trigger RAISE(ABORT) violations via error message substring"
slug: sqlite-trigger-raise-abort-message-match
date: 2026-05-09
phase_id: "6"
generalizable: true
one_sentence_lesson: "SQLite `RAISE(ABORT)` from immutability triggers returns error code 1 (SQLITE_ERROR) with no unique extended code — detect it through sqlx by substring-matching the error message against the trigger's string literal."
tags: [rust, sqlite, sqlx, triggers, error-handling]
---

## Context

Phase 6 added `BEFORE UPDATE` and `BEFORE DELETE` immutability triggers to `audit_log` (ADR-0009). Integration tests needed to verify these triggers fire correctly and return a recognisable error, not a generic storage failure. The initial assumption was that sqlx would expose a dedicated error variant or unique error code for trigger-level aborts.

## What Happened

SQLite's `RAISE(ABORT, 'message')` returns `SQLITE_ERROR` (code 1) regardless of the message content. There is no extended error code, no unique return code, and no dedicated variant in sqlx's error model. The only reliable discriminant is the string literal passed to `RAISE`.

The pattern used:

```rust
// In the trigger definition (migration):
SELECT RAISE(ABORT, 'audit_log rows are immutable (architecture §6.6)');

// In the adapter error-mapping code:
if let Some(db_err) = e.as_database_error() {
    if db_err.message().contains("audit_log rows are immutable") {
        return Err(StorageError::Integrity(db_err.message().to_owned()));
    }
}
```

Choose the trigger message string carefully — it becomes part of the public error-detection contract. Keep it stable and unique enough to avoid false matches against other errors.

## Lesson

> SQLite `RAISE(ABORT)` from immutability triggers returns error code 1 (SQLITE_ERROR) with no unique extended code — detect it through sqlx by substring-matching the error message against the trigger's string literal.

## Applies When

- Writing adapter code that must distinguish a trigger-enforced constraint violation from a generic DB error
- Adding `BEFORE UPDATE`/`BEFORE DELETE` immutability triggers to any table
- Writing integration tests that assert trigger fires produce a specific `StorageError` variant

## Does Not Apply When

- `CHECK` constraint violations — those return `SQLITE_CONSTRAINT` (code 19) and sqlx exposes `DatabaseError::kind() == ErrorKind::CheckConstraintViolation`
- Unique index violations — use `ErrorKind::UniqueViolation` instead

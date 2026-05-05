---
name: SQLite extended error codes must be masked with 0xFF
description: SQLite extended codes like BUSY_RECOVERY (261) and BUSY_SNAPSHOT (517) won't match the base code (5) unless you mask with 0xFF
type: solution
category: runtime-errors
phase_id: onboard-git-history
source_commit: 8a5180d
source_date: 2026-05-03
one_sentence_lesson: Always match SQLite error codes on `code & 0xFF` — extended codes (BUSY_RECOVERY=261, BUSY_SNAPSHOT=517) include the base code in their low 8 bits and won't match a bare `5` or `6`.
---

## Problem

`capability_store.rs` matched SQLite error codes with a bare integer match:

```rust
match code {
    5 | 6 => StorageError::SqliteBusy { retries: 0 },
    ...
}
```

SQLite extended error codes are formed as `(base_code) | (extended_code << 8)`:
- `SQLITE_BUSY` = 5
- `SQLITE_BUSY_RECOVERY` = 261 (= 5 | (1 << 8))
- `SQLITE_BUSY_SNAPSHOT` = 517 (= 5 | (2 << 8))

The match on `5` only caught the base code; extended variants fell through to the catch-all, masking retryable busy errors as generic failures.

## Fix

```rust
// Mask to primary error code so extended codes are caught alongside base codes.
match code & 0xFF {
    5 | 6 => StorageError::SqliteBusy { retries: 0 },
    11 => StorageError::Sqlite { kind: SqliteErrorKind::Corrupt, source: e },
    19 => StorageError::Sqlite { kind: SqliteErrorKind::Constraint, source: e },
    _ => StorageError::Sqlite { kind: SqliteErrorKind::Other, source: e },
}
```

## When to apply

Any SQLite error code matching in Rust (sqlx, rusqlite, or raw FFI). If you're switching on the raw code integer, always mask first.

## Category

runtime-errors

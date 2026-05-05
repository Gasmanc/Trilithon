---
name: sync_all() before atomic rename for file durability
description: An atomic write (write-to-temp then rename) must call sync_all() before the rename or the data may not be on disk after a crash
type: solution
category: best-practices
phase_id: onboard-git-history
source_commit: 8a5180d
source_date: 2026-05-03
one_sentence_lesson: When writing a file atomically via a temp file + rename, call sync_all() before the rename — the rename itself may survive a crash while the file data hasn't been flushed to disk.
---

## Problem

`installation_id.rs` wrote a new installation ID using a write-then-rename pattern but did not call `sync_all()` before the rename. On a crash between write and flush, the rename could succeed (persisting the new filename) while the data remained in the OS buffer cache — leaving a zero-byte or truncated file on the next boot.

## Fix

```rust
file.write_all(id.as_bytes())?;
file.sync_all()?;          // guarantee data is on disk before rename
drop(file);
fs::rename(&tmp_path, &target_path)?;
```

## When to apply

Any atomic file write pattern:
1. Open a temp file in the same directory as the target
2. Write contents
3. `sync_all()` ← must not skip this
4. Rename temp → target

The same directory requirement ensures the rename is on the same filesystem, making it atomic at the OS level.

## Category

best-practices

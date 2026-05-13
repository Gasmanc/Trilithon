---
track: bug
problem_type: race-condition
root_cause: concurrency-gap
resolution_type: refactored
severity: high
title: "Drop impl releasing a lock guard must complete synchronously"
slug: drop-guarded-resource-release-sync
date: 2026-05-13
phase_id: "7"
generalizable: true
one_sentence_lesson: "When a Drop impl releases a resource that guards a subsequent lock, the release must complete synchronously before Drop returns — fire-and-forget background tasks create a window where the next holder sees a spurious conflict"
tags: [rust, tokio, drop, lock, advisory-lock, block_in_place]
---

## Context

`AcquiredLock` holds an advisory lock row in SQLite that serializes concurrent `apply()` calls. Its `Drop` impl must delete the lock row so the next caller can acquire it. The original impl spawned a new `tokio::runtime::Builder::new_current_thread()` to run the async `DELETE` from within the sync `drop()`.

## What Happened

Constructing a new Tokio runtime inside `drop()` panics when called from within an existing async context. The fix replaced the nested runtime with `tokio::task::block_in_place`, which suspends the current async task and runs the blocking `DELETE` synchronously on the same thread. This guarantees the lock row is deleted before `Drop` returns and before the in-process `Mutex` guard drops — closing the window where the next `apply()` caller could see the advisory lock still present in SQLite.

## Lesson

> When a Drop impl releases a resource that guards a subsequent lock, the release must complete synchronously before Drop returns — fire-and-forget background tasks create a window where the next holder sees a spurious conflict

## Applies When

- A `Drop` impl must perform async I/O (DB delete, network close, file unlock) to release an external resource
- That external resource gates access for the next concurrent caller (advisory lock, lease, semaphore row)
- The code runs inside a Tokio async context where spawning a new runtime would panic

## Does Not Apply When

- The resource release is best-effort (e.g. TTL-based expiry handles non-release) and spurious conflicts are acceptable
- The `Drop` is called from a thread that never runs inside a Tokio runtime (pure sync code)

---
track: bug
problem_type: data-loss
root_cause: missing-error-handling
resolution_type: refactored
severity: high
title: "Track tokio::spawn handles in JoinSet to drain tasks on shutdown"
slug: joinset-drain-spawned-tasks-shutdown
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "Every tokio::spawn handle must be tracked in a JoinSet (or Vec of JoinHandles) so tasks can be drained on shutdown — discarding handles means background work is aborted rather than cleanly wound down."
tags: [rust, tokio, async, shutdown, joinset, graceful-shutdown, tasks]
---

## Context

A Tokio-based daemon spawns multiple background loops (`integrity_loop`, `reconnect_loop`) during startup and needs to drain them cleanly within a budget window when a shutdown signal arrives.

## What Happened

`run_with_shutdown` spawned background tasks via `tokio::spawn` but discarded the returned `JoinHandle`s. Only the main `daemon_loop` handle was awaited during shutdown. When the runtime was dropped after the drain budget expired, all background tasks were abruptly killed at their last yield point. For Phase 1 this was harmless (empty loops), but any real I/O or DB write in those loops would be truncated. The fix replaced bare `tokio::spawn` calls with a `JoinSet`, which was then awaited (with timeout) during the shutdown phase to ensure all tasks completed or were explicitly cancelled.

## Lesson

> Every tokio::spawn handle must be tracked in a JoinSet (or Vec of JoinHandles) so tasks can be drained on shutdown — discarding handles means background work is aborted rather than cleanly wound down.

## Applies When

- Spawning any Tokio task that performs I/O, DB writes, or state mutations
- Building a daemon that needs graceful shutdown with a time budget
- Any code that calls `tokio::spawn` and ignores the return value

## Does Not Apply When

- The task is fire-and-forget and data loss on abort is explicitly acceptable (e.g. a metrics flush that can be missed)
- The task holds no resources and will be dropped cleanly by the runtime

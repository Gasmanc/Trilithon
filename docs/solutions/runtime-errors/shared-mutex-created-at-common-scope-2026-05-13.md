---
track: bug
problem_type: correctness
root_cause: shared-dependency-assumption
resolution_type: refactored
severity: high
title: "Create shared mutex at the scope that owns both consumers"
slug: shared-mutex-created-at-common-scope
date: 2026-05-13
phase_id: "8"
generalizable: true
one_sentence_lesson: "A mutex shared between two subsystems must be created at the scope that owns both subsystems — creating it inside one subsystem makes it permanently invisible to the other"
tags: [rust, tokio, mutex, arc, concurrency, dependency-injection]
---

## Context

`DriftDetector` and `CaddyApplier` are meant to share an `apply_mutex` so the
drift detector skips its tick when a config-apply is in flight
(`SkippedApplyInFlight`). Both are constructed in `run_with_shutdown` and
injected into background tasks.

## What Happened

The `apply_mutex` was created inside `build_drift_detector`, making it
private to the detector. `CaddyApplier` (wired in a later phase) would
receive its own separate mutex. Because the two `Arc<Mutex<()>>` instances
are distinct allocations, `try_lock()` on the detector's mutex can never
observe a lock held by the applier — the `SkippedApplyInFlight` arm became
permanently dead code, silently breaking the concurrency contract.

The fix: create `apply_mutex` at `run_with_shutdown` scope (the common owner
of both subsystems) and pass `Arc::clone` into each constructor. Both now
share the same underlying mutex.

## Lesson

> A mutex shared between two subsystems must be created at the scope that owns both subsystems — creating it inside one subsystem makes it permanently invisible to the other

## Applies When

- Two independently-constructed services need to coordinate via a shared lock
  (e.g. a writer and a watcher that must not run concurrently)
- Reviewing any `Arc<Mutex<T>>` field that is constructed inside a helper
  function rather than at the injection site
- Adding a new lock to enforce mutual exclusion between existing services

## Does Not Apply When

- The mutex guards internal state that only one subsystem ever touches
- The two consumers are in the same task and can use a single `&mut` reference
  instead of a shared `Arc`

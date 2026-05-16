---
track: bug
problem_type: correctness
root_cause: wrong-ordering
resolution_type: refactored
severity: high
title: "Persist to storage only after the confirming outcome is observed"
slug: persist-after-confirmed-outcome
date: 2026-05-16
phase_id: "9"
generalizable: true
one_sentence_lesson: "Side-effectful persistence (DB insert) must happen only after the confirming outcome is observed — never before an operation whose failure should leave no trace"
tags: [sqlite, storage, concurrency, correctness]
---

## Context

The mutation handler called `storage.insert_snapshot(snapshot)` unconditionally before calling `applier.apply()`. If the applier returned `OptimisticConflict`, `LockContested`, or `Failed`, the snapshot row was already in storage with no rollback.

## What Happened

`latest_desired_state()` would return the phantom snapshot on the next request, causing subsequent mutations to compute diffs from a state Caddy had never seen — a silent data-consistency violation. The fix moved `insert_snapshot` inside the `ApplyOutcome::Succeeded` arm, so the row only exists after a confirmed apply.

## Lesson

> Side-effectful persistence (DB insert) must happen only after the confirming outcome is observed — never before an operation whose failure should leave no trace

## Applies When

- A write to a persistent store is paired with a fallible external or concurrent operation
- The stored row would be read by subsequent operations and influence their behavior
- There is no compensating transaction or rollback mechanism covering both sides

## Does Not Apply When

- The persistence is the primary operation and the external call is a side-effect (reverse dependency order)
- Both operations are wrapped in a distributed transaction that rolls back atomically

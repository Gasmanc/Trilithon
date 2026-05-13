---
track: bug
problem_type: correctness
root_cause: incorrect-algorithm
resolution_type: refactored
severity: high
title: "Rollback CAS expected value must be current state, not target state"
slug: rollback-cas-use-current-not-target-version
date: 2026-05-13
phase_id: "7"
generalizable: true
one_sentence_lesson: "Rollback operations must use the current state as the CAS expected value, not the target state's version — the invariant is 'I own the current slot', not 'I own the target slot'"
tags: [rust, cas, rollback, sqlite, apply-path]
---

## Context

The `rollback()` function reverts `applied_config_version` to a prior snapshot by issuing a CAS operation: "if the current version is X, set it to Y". The expected value (X) was taken from `snapshot.config_version` — the version of the target snapshot being rolled back to.

## What Happened

Using the target snapshot's version as the CAS expected value means the rollback only succeeds if `applied_config_version` already equals the target version — which is never true (that's what the rollback is trying to achieve). The correct expected value is the *current* `applied_config_version` read from storage at rollback time. The invariant for a CAS write is "I observed the current value and I own the right to change it", not "I know what I want it to become". The fix reads `current_config_version` from storage and uses that as the expected value.

## Lesson

> Rollback operations must use the current state as the CAS expected value, not the target state's version — the invariant is 'I own the current slot', not 'I own the target slot'

## Applies When

- A CAS operation transitions from a current observed value to a new desired value
- The operation is a rollback, revert, or undo that moves state backward to a prior version
- The current value is read from an external source (DB, cache) rather than held in-memory

## Does Not Apply When

- The CAS operation uses a monotonic counter where the caller already holds the current value in-process
- The expected value is passed in by the caller who is responsible for observing and providing it

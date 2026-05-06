---
track: bug
problem_type: crash
root_cause: missing-error-handling
resolution_type: guard-added
severity: high
title: "Version counters must use checked_add to avoid overflow panic"
slug: version-counter-checked-add-overflow
date: 2026-05-06
phase_id: "4"
generalizable: true
one_sentence_lesson: "Version counters that use unchecked integer addition will panic at i64::MAX — use checked_add and map None to a domain error so the caller can handle it"
tags: [rust, overflow, version, checked_add, arithmetic]
---

## Context

Phase 4 introduced optimistic concurrency on `DesiredState` via an `i64` version field. Each `apply_mutation` call incremented the version with `state.version + 1` using the default `+` operator. In Rust release builds, integer overflow panics in debug mode and wraps silently in release mode depending on overflow checks configuration.

## What Happened

An overflowed version could coincidentally match a stale in-flight mutation's `expected_version`, causing the conflict check (`expected != state.version`) to accept a stale mutation as current. The fix replaced the bare `+ 1` with `state.version.checked_add(1).ok_or(MutationError::Forbidden { reason: ForbiddenReason::VersionOverflow })?` and added the `VersionOverflow` variant to `ForbiddenReason`. The change costs one arithmetic check per mutation and surfaces a well-defined error instead of a panic or silent wrap.

## Lesson

> Version counters that use unchecked integer addition will panic at i64::MAX — use checked_add and map None to a domain error so the caller can handle it

## Applies When

- A counter or version field is incremented with bare `+` on a primitive integer type
- The counter is used for conflict detection, ordering, or idempotency checks
- The system is long-running and the counter could theoretically reach the integer maximum

## Does Not Apply When

- The counter is bounded by domain constraints that guarantee it never approaches overflow (e.g. `u8` index into a fixed-size array of 10 items)
- The counter uses a type that saturates rather than wrapping (e.g. `Saturating<u64>`)

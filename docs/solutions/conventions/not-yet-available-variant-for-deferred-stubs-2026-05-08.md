---
track: knowledge
problem_type: api-contract
root_cause: wrong-abstraction
resolution_type: refactored
severity: high
title: "Use a dedicated NotYetAvailable variant for deferred stub methods"
slug: not-yet-available-variant-for-deferred-stubs
date: 2026-05-08
phase_id: "onboard-git-history"
source_commit: 64d186b
generalizable: true
one_sentence_lesson: "Stub methods that return 'comes in Phase N' errors should use a dedicated NotYetAvailable variant, not an existing variant like Migration — callers must be able to distinguish deferred-feature errors from real startup or runtime failures."
tags: [rust, error-handling, thiserror, stub, api-contract, enum]
---

## Context

Phase 2 added five `SqliteStorage` methods whose backing tables did not exist yet (they were scheduled for later phases). The stub implementations returned `StorageError::Migration { version: 0, detail: "… arrives in Phase N" }` — borrowing the Migration variant because it was the closest available structural match.

## What Happened

`StorageError::Migration` was already used to signal genuine migration-sequence failures (tables missing because migrations had not been applied, wrong migration order, etc.). Overloading it for "not yet implemented" stubs made those two failure modes indistinguishable to callers. `From<StorageError> for ExitCode` mapped `Migration` to `StartupPreconditionFailure` (exit 3), so calling an unimplemented stub would exit with a precondition-failure code rather than the semantically correct `InvalidInvocation` (exit 64).

The fix added `StorageError::NotYetAvailable { reason: String }` and mapped it to exit 64, then updated all five stubs to use it.

## Lesson

> Stub methods that return 'comes in Phase N' errors should use a dedicated NotYetAvailable variant, not an existing variant like Migration — callers must be able to distinguish deferred-feature errors from real startup or runtime failures.

## Applies When

- Adding stub implementations of a trait for features deferred to a future phase
- The existing error enum already has variants with production semantics you'd be tempted to borrow
- The caller (e.g. an exit-code mapper or error logger) treats different variants differently

## Does Not Apply When

- The missing feature has no callers yet — in that case a compile-time `todo!()` or `unimplemented!()` is cleaner than a runtime variant
- The stub is test-only and will never reach production code paths

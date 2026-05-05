---
track: bug
problem_type: api-contract
root_cause: hidden-coupling
resolution_type: refactored
severity: high
title: "All Storage trait implementations must return identical error variants for equivalent failures"
slug: storage-trait-error-variant-parity
date: 2026-05-05
phase_id: "5"
generalizable: true
one_sentence_lesson: "When a Storage trait has multiple implementations (in-memory for tests, SQLite for production), every impl must return the same error variant for the same failure condition — divergence makes tests pass while production silently breaks."
tags: [rust, storage, trait, testing, api-contract, in-memory]
---

## Context

`SqliteStorage` and `InMemoryStorage` both implement the `Storage` trait. During Phase 5, `SqliteStorage` was updated to return `StorageError::SnapshotHashCollision` when a duplicate snapshot id is inserted with a different body. `InMemoryStorage` continued returning a different (older) error variant for the same condition.

## What Happened

Tests that used `InMemoryStorage` continued to pass because they asserted on the old variant. Any test written against `InMemoryStorage` that expected `SnapshotHashCollision` would fail, but tests that expected the old variant gave false confidence. In production, the `SqliteStorage` path would produce a different error, breaking any caller that had been validated against `InMemoryStorage`.

The fix: update `InMemoryStorage::insert_snapshot` to produce `SnapshotHashCollision` under the same conditions as `SqliteStorage`. The rule: every branch in every `Storage` impl must mirror the error taxonomy of every other impl.

## Lesson

> When a Storage trait has multiple implementations (in-memory for tests, SQLite for production), every impl must return the same error variant for the same failure condition — divergence makes tests pass while production silently breaks.

## Applies When

- Any project with a test double (in-memory, mock) that implements the same trait as a production adapter
- After updating error handling in one Storage impl — audit every other impl for the same case
- Code review of new `Storage` trait methods: verify error variants are documented in the trait and matched by all impls

## Does Not Apply When

- The in-memory implementation is only used in tests and the test assertions explicitly check for "any error" rather than a specific variant
- The trait explicitly documents that error variants are implementation-defined

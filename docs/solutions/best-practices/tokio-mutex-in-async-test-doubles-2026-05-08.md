---
track: knowledge
problem_type: test-reliability
root_cause: wrong-abstraction
resolution_type: refactored
severity: high
title: "Use tokio::sync::Mutex not std::sync::Mutex in async test doubles"
slug: tokio-mutex-in-async-test-doubles
date: 2026-05-08
phase_id: "onboard-git-history"
source_commit: 28f9d41
generalizable: true
one_sentence_lesson: "Async test doubles that implement async traits should use tokio::sync::Mutex, not std::sync::Mutex — a panic inside one async test poisons std's mutex and cascade-fails every subsequent test that tries to lock it."
tags: [rust, tokio, async, testing, mutex, test-double, concurrency]
---

## Context

`InMemoryStorage` used `std::sync::Mutex` to guard its internal collections. The choice made sense at first: `core` was supposed to be free of Tokio in production, so using `std` primitives felt correct. The double was compiled only in `#[cfg(test)]` builds.

## What Happened

`std::sync::Mutex` is poisoned when a thread panics while holding the lock. In an async Tokio test harness, `.lock().expect("lock poisoned")` will panic on *every subsequent test* after any single test panics mid-hold — even unrelated tests that happened to use the same storage instance. The error messages ("snapshots lock poisoned") were confusing because they made unrelated tests appear broken.

The fix swapped all internal `Mutex<_>` fields to `tokio::sync::Mutex<_>` and changed all `lock().expect(...)` calls to `.lock().await`. `tokio::sync::Mutex` does not implement poisoning — a panic in one test releases the lock without tainting it for subsequent holders.

## Lesson

> Async test doubles that implement async traits should use `tokio::sync::Mutex`, not `std::sync::Mutex` — a panic inside one async test poisons `std`'s mutex and cascade-fails every subsequent test that tries to lock it.

## Applies When

- Writing a test double (in-memory implementation) for an async trait
- The test suite runs under `tokio::test` and multiple tests share the same double instance
- You want clean, isolated test failures rather than cascading lock-poisoned panics

## Does Not Apply When

- The double is used only in single-threaded sync tests (no async runtime)
- The struct is a production type that must remain free of Tokio — use `parking_lot::Mutex` (no poisoning, no async runtime required) instead

---
track: bug
problem_type: correctness
root_cause: hidden-coupling
resolution_type: refactored
severity: high
title: "Boolean guard on empty collection is insufficient — verify the collection is actually populated"
slug: guard-not-sufficient-verify-data-flows
date: 2026-05-13
phase_id: "7"
generalizable: true
one_sentence_lesson: "After wiring a feature behind a boolean guard (is_empty check), always trace back through the call chain to verify the data that populates the collection is actually being produced"
tags: [rust, wiring, observer, guard]
---

## Context

The TLS issuance observer (`TlsIssuanceObserver::observe`) is spawned inside the apply path when there are hostnames to watch. The call site guarded the spawn with `if !hostnames.is_empty()` to skip spawning when there's nothing to observe.

## What Happened

The `hostnames` collection was derived from `desired_state.routes`, but the extraction code was never written — only the guard was in place. The spawn was called with an empty `Vec<String>` on every apply, meaning the observer was never invoked and the guard was silently always true. The fix required tracing back to where `hostnames` should have been populated (iterating enabled routes, extracting `HostPattern::Exact` and `HostPattern::Wildcard` values) and wiring that extraction before the guard.

## Lesson

> After wiring a feature behind a boolean guard (is_empty check), always trace back through the call chain to verify the data that populates the collection is actually being produced

## Applies When

- A new subsystem is spawned or invoked conditionally based on a collection's emptiness
- The collection is derived from a data structure that exists but whose traversal logic hasn't been written yet
- The feature has no observable failure mode when the guard short-circuits (it silently no-ops)

## Does Not Apply When

- The guard protects against a legitimately empty collection in normal operation (e.g. no enabled routes is a valid state)
- The collection is populated by the caller, not derived internally at the call site

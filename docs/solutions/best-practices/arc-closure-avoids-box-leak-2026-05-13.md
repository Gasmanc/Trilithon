---
track: bug
problem_type: resource-leak
root_cause: wrong-ownership-model
resolution_type: refactored
severity: high
title: "Capture Arc clones in closures instead of Box::leak for static refs"
slug: arc-closure-avoids-box-leak
date: 2026-05-13
phase_id: "8"
generalizable: true
one_sentence_lesson: "Avoid Box::leak in daemon startup paths by capturing Arc clones inside the closure instead of deriving 'static references from leaked allocations"
tags: [rust, arc, box-leak, static, memory, daemon]
---

## Context

`AuditWriter::new` requires a `SecretsRedactor<'static>`, which in turn
requires `&'static SchemaRegistry` and `&'static dyn CiphertextHasher`. In
`build_drift_detector`, these were obtained via `Box::leak`, permanently
allocating the values and deriving `'static` references from them.

## What Happened

`Box::leak` is a valid Rust idiom for intentionally-permanent allocations,
but using it in a daemon startup function has two problems: (1) the allocation
is truly never reclaimed — in a test harness that starts and stops the daemon
repeatedly, leaked memory accumulates; (2) it signals to readers that the
values are intentionally immortal, obscuring that `AuditWriter` is simply
using `'static` as a proxy for "lives as long as the writer," which `Arc`
satisfies equally well.

The fix: add `AuditWriter::new_with_arcs(registry: Arc<SchemaRegistry>,
hasher: Arc<dyn CiphertextHasher>)`. The constructor clones both `Arc`s into
a closure (`move |value| { let r = SecretsRedactor::new(&registry, &*hasher); ... }`).
The closure captures `Arc`s by value, so `SecretsRedactor` is created fresh
per call with references that live for the closure invocation. No `'static`
lifetime is needed; no memory leaks.

## Lesson

> Avoid Box::leak in daemon startup paths by capturing Arc clones inside the closure instead of deriving 'static references from leaked allocations

## Applies When

- A struct requires `&'static T` but `T` is only needed for the lifetime of
  the struct — wrap `T` in `Arc` and capture it in a closure instead
- Writing a long-lived writer, processor, or handler that wraps a
  configuration or registry object
- Reviewing daemon startup code that contains `Box::leak` — ask whether the
  value truly needs to live forever or just "as long as the consumer"

## Does Not Apply When

- The value genuinely must be immortal (e.g. a static dispatch table
  registered at program start and never deallocated)
- The closure overhead (re-creating `SecretsRedactor` per call) is
  measurably on the hot path — in that case, store an `Arc<SecretsRedactor<'static>>`
  created once from the leaked refs

---
track: bug
problem_type: data-loss
root_cause: wrong-type-assumption
resolution_type: guard-added
severity: high
title: "Check serde_json Number variant before calling as_f64 to avoid silent integer truncation"
slug: serde-json-number-variant-before-f64-cast
date: 2026-05-05
phase_id: "5"
generalizable: true
one_sentence_lesson: "When canonicalising serde_json::Value numbers, call `n.is_f64()` before `n.as_f64()` — i64/u64 values silently lose precision when routed through f64 for integers larger than 2^53."
tags: [rust, serde_json, canonicalization, data-loss, integers]
---

## Context

Phase 5 introduced a canonical JSON serialiser for `DesiredState` whose job is to produce a deterministic byte representation for SHA-256 content addressing. The serialiser walked `serde_json::Value` trees and normalised numbers by calling `n.as_f64()` uniformly on every numeric node before re-encoding.

## What Happened

`serde_json::Value::Number` stores integers as i64 or u64 internally and only promotes to f64 when the source JSON contained a decimal point. Calling `as_f64()` on an i64 or u64 that exceeds 2^53 silently returns a rounded float, producing a different byte string than the original. The canonicaliser therefore corrupted route IDs and port numbers above 9_007_199_254_740_992 — values that round-trip correctly through plain `serde_json` but not through the float-normalisation path.

The fix: check `n.is_f64()` first. Only apply float normalisation when the underlying variant is already a float. Leave i64 and u64 values untouched.

```rust
// Before (corrupts large integers):
if let Some(f) = n.as_f64() { Value::from(f) } else { Value::Number(n.clone()) }

// After (preserves integer precision):
if n.is_f64() {
    Value::from(n.as_f64().unwrap())
} else {
    Value::Number(n.clone())
}
```

## Lesson

> When canonicalising serde_json::Value numbers, call `n.is_f64()` before `n.as_f64()` — i64/u64 values silently lose precision when routed through f64 for integers larger than 2^53.

## Applies When

- Writing a JSON canonicaliser that must round-trip integers faithfully
- Any code that maps over `serde_json::Value::Number` and calls `as_f64()`
- Content-addressing or hashing schemes that must be deterministic across numeric types

## Does Not Apply When

- The domain guarantees all numeric values are IEEE-754 doubles (e.g. JSON from JavaScript `JSON.stringify`)
- The integers in question are small enough that f64 represents them exactly (< 2^53)

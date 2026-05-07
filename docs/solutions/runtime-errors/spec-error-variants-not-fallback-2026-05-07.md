---
track: knowledge
problem_type: api-contract
title: "Implement every spec-required error variant — don't rely on generic fallbacks"
slug: spec-error-variants-not-fallback
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "Spec-required error variants must be implemented even when a generic fallback (MalformedToml) would technically work — each distinct failure mode needs its own variant for callers and tests to pattern-match."
tags: [rust, error-handling, thiserror, api-contract, spec, enum]
---

## Context

A config loader's `ConfigError` enum was specified to include `BindAddressInvalid { value: String }` for bad bind-address values. The implementation omitted this variant entirely. Bad addresses fell through to `MalformedToml`, which technically surfaced an error to the user but conveyed the wrong semantics.

## What Happened

The missing variant had no compile-time enforcement. Integration tests that matched on `ConfigError::BindAddressInvalid` could not be written, so the test coverage gap was also invisible. The fix added the variant and a pre-deserialization step that tried `SocketAddr::parse` on the `server.bind` value after env overrides were applied, returning `BindAddressInvalid { value }` on failure. A test then asserted the correct variant.

## Lesson

> Spec-required error variants must be implemented even when a generic fallback (MalformedToml) would technically work — each distinct failure mode needs its own variant for callers and tests to pattern-match.

## Applies When

- Implementing an error enum where the spec lists specific variants
- Writing config or input validation where different failure modes need distinct handling
- Reviewing a PR where a catch-all arm (`_ => SomeGenericError`) is used instead of a specific variant

## Does Not Apply When

- The spec explicitly says to use a generic error type
- The failure mode genuinely has no distinct recovery path (all errors are fatal with the same user message)

---
track: bug
problem_type: correctness
root_cause: hidden-coupling
resolution_type: refactored
severity: high
title: "Two paths that must agree byte-for-byte on an enum's string form need a single typed source of truth"
slug: enum-string-single-source-hash-chain
date: 2026-05-13
phase_id: "6"
generalizable: true
one_sentence_lesson: "Whenever two code paths must agree byte-for-byte on the string form of an enum (here: SQL storage and the canonical-JSON used for hash chaining), define a single `as_*_str` method on the enum and call it from both sites — never rely on `format!(\"{:?}\")` matching the SQL string, because the first multi-word variant silently breaks the invariant."
tags: [rust, audit, hash-chain, enums, correctness]
---

## Context

The audit log uses a hash chain for immutability: each row stores a `prev_hash` derived from the previous row's canonical JSON representation. Both the SQL storage layer (`actor_kind_str` / `outcome_str` column values) and the `canonical_json_for_audit_hash` builder must produce identical string representations of `ActorKind` and `AuditOutcome` — any divergence silently breaks the chain.

## What Happened

Both sites were using `format!("{:?}", value).to_lowercase()` to produce the string. For single-word variants like `User`, `Token`, `Ok` this produces the expected strings (`"user"`, `"token"`, `"ok"`). However, for any future multi-word variant (e.g. `ServiceAccount`), `{:?}` produces `"ServiceAccount"` which `.to_lowercase()` converts to `"serviceaccount"` — not `"service_account"` or `"service-account"`, and not necessarily what the SQL column expects. The two sites had no compile-time or runtime mechanism to detect drift. The fix was to add `ActorKind::as_audit_str` and `AuditOutcome::as_audit_str` `const fn` methods on the enums themselves and route both sites through them.

## Lesson

> Whenever two code paths must agree byte-for-byte on the string form of an enum (here: SQL storage and the canonical-JSON used for hash chaining), define a single `as_*_str` method on the enum and call it from both sites — never rely on `format!("{:?}")` matching the SQL string, because the first multi-word variant silently breaks the invariant.

## Applies When

- An enum's string form appears in both a storage column and a hash or checksum computation
- Two or more separate code paths must produce identical byte sequences from the same typed value
- A new enum variant would require updating both sites consistently

## Does Not Apply When

- Only one code path uses the string form (no byte-agreement requirement)
- The string form is only used for display or logging, where divergence has no correctness impact

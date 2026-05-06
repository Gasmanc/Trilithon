---
track: bug
problem_type: api-contract
root_cause: insufficient-validation
resolution_type: validation-added
severity: critical
title: "Identifier newtypes must enforce format at TryFrom boundaries"
slug: newtype-enforce-format-at-tryfrom-boundary
date: 2026-05-06
phase_id: "4"
generalizable: true
one_sentence_lesson: "Identifier newtypes whose docs claim ULID format must enforce it at TryFrom boundaries — deserialization alone is insufficient if callers can bypass the newtype constructor"
tags: [rust, newtype, ulid, validation, serde, deserialization]
---

## Context

Phase 4 introduced `RouteId`, `UpstreamId`, `PolicyId`, `PresetId`, and `MutationId` as `pub(String)` newtypes. The `new()` constructor produced valid ULIDs, and the doc comments stated the inner string must be a valid ULID. However, serde `Deserialize` was derived directly, which accepts any string.

## What Happened

A reviewer identified that `RouteId("../../etc/passwd")`, `RouteId("")`, or `RouteId("not-a-ulid")` all deserialise successfully and enter `DesiredState` as BTreeMap keys. These raw strings flow into hint messages, log lines, and eventually into storage key construction in Phase 5. The constructor invariant was documented but not enforced at the deserialization boundary. Adding `TryFrom<String>`/`TryFrom<&str>` impls that check the ULID character set (`[0-9A-Z]{26}`, 26 chars) and wiring them to a `#[serde(try_from = "String")]` attribute closed the gap.

## Lesson

> Identifier newtypes whose docs claim ULID format must enforce it at TryFrom boundaries — deserialization alone is insufficient if callers can bypass the newtype constructor

## Applies When

- A newtype wraps `String` and the inner string must satisfy a format invariant (ULID, UUID, slug, etc.)
- The newtype is serialised/deserialised with serde derive
- The newtype flows into storage keys, log lines, or hint messages where format violations could cause security or correctness issues

## Does Not Apply When

- The newtype wraps a primitive that has its own type-level invariants (e.g. `NonZeroU64`, `Ipv4Addr`)
- The format invariant is intentionally permissive and any string is acceptable

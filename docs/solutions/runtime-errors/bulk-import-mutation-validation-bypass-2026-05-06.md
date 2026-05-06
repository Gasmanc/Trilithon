---
track: bug
problem_type: api-contract
root_cause: insufficient-validation
resolution_type: validation-added
severity: high
title: "Bulk-import mutations bypass individual create validators"
slug: bulk-import-mutation-validation-bypass
date: 2026-05-06
phase_id: "4"
generalizable: true
one_sentence_lesson: "Bulk-import mutations must run the same pre-condition validators as individual create mutations, including intra-batch duplicate detection, or they become a validation bypass vector"
tags: [rust, mutation, validation, import, pre-conditions]
---

## Context

Phase 4 introduced `ImportFromCaddyfile` as a mutation that merges routes and upstreams from a parsed Caddyfile into `DesiredState`. `CreateRoute` and `CreateUpstream` each had individual pre-condition validators (hostname check, duplicate ID check, upstream reference check). `ImportFromCaddyfile` returned `Ok(())` from `pre_conditions` unconditionally.

## What Happened

Because `apply_import_caddyfile` used `BTreeMap::insert`, it silently overwrote any existing route or upstream with a matching ID. An import could also write routes whose hostnames contained invalid characters, or routes referencing upstream IDs absent from both the current state and the import batch. The fix added `check_import_caddyfile()` in `validate.rs` that (1) validates hostnames on all imported routes, (2) checks upstream references against current state unioned with the import batch, and (3) detects intra-import duplicate IDs via a `BTreeSet` before applying any insert.

## Lesson

> Bulk-import mutations must run the same pre-condition validators as individual create mutations, including intra-batch duplicate detection, or they become a validation bypass vector

## Applies When

- A mutation variant processes a collection of entities in one operation (import, bulk-create, batch upsert)
- Individual create mutations for the same entity type have pre-condition validators
- The batch operation calls `BTreeMap::insert` or equivalent without conflict handling

## Does Not Apply When

- The bulk mutation has explicitly documented upsert-or-replace semantics and callers are aware of that contract
- The mutation is internal-only and entity integrity is guaranteed by the caller's context

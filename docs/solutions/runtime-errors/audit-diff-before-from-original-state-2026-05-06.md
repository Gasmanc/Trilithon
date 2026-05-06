---
track: bug
problem_type: correctness
root_cause: shared-dependency-assumption
resolution_type: refactored
severity: high
title: "Audit diff before-state must be captured from the original state"
slug: audit-diff-before-from-original-state
date: 2026-05-06
phase_id: "4"
generalizable: true
one_sentence_lesson: "Audit diff before-state must be captured from the original state before mutation, not from the clone being mutated — reading from new_state always produces identical before/after pairs"
tags: [rust, audit, diff, mutation, state]
---

## Context

Phase 4 introduced `MutationOutcome.diff` containing before/after `DiffChange` entries for every mutated field. Apply helpers like `apply_route_patch` and `apply_upstream_patch` correctly accepted the original immutable `state` to read `before` values. Policy and config helpers (`policy_attachment_preamble`, `apply_set_global_config`, `apply_set_tls_config`) captured `before` from `new_state` — the mutable clone.

## What Happened

Coincidentally, both helpers captured `before` before mutating `new_state`, so the values were correct in isolation. However, the hidden ordering dependency became a latent bug: any future refactor that mutates `new_state` before calling these helpers, or any compound mutation that shares `new_state` across sub-operations, would produce a `before` equal to the post-mutation value — making the audit diff appear as a no-op. The fix passed the original immutable `state: &DesiredState` into all apply helpers and consistently read `before` from it.

## Lesson

> Audit diff before-state must be captured from the original state before mutation, not from the clone being mutated — reading from new_state always produces identical before/after pairs

## Applies When

- An apply function produces a before/after diff for audit or display
- The function receives both an original immutable state and a mutable clone to modify
- The before-value is captured inside the same function that performs the mutation

## Does Not Apply When

- The diff is computed by diffing the final states after all mutations complete (snapshot-based diffing rather than inline capture)

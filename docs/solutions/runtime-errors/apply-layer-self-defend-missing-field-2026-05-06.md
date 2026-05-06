---
track: bug
problem_type: api-contract
root_cause: missing-error-handling
resolution_type: guard-added
severity: high
title: "Apply functions must error when a required field is absent"
slug: apply-layer-self-defend-missing-field
date: 2026-05-06
phase_id: "4"
generalizable: true
one_sentence_lesson: "An apply function that silently no-ops when a required field is absent produces a successful mutation with no observable effect — always propagate an error so the caller knows the operation was a no-op"
tags: [rust, mutation, apply, validation, layering, self-defending]
---

## Context

Phase 4's `apply_upgrade_policy` used `if let Some(attachment) = route.policy_attachment.as_mut()` to perform the version bump. The pre-condition layer (`check_upgrade_policy`) guards against a missing attachment, so in normal flow the `None` branch is unreachable. However, the two layers are structurally decoupled.

## What Happened

If `apply_upgrade_policy` was ever called without having `pre_conditions` run first — for example, from a future Phase 7 rollback path or a batch processor — a missing attachment would cause the `if let` arm to be skipped silently. The function returned `Ok(vec![DiffChange { before: None, after: None }])`, incremented the version counter, and produced a valid-looking `MutationOutcome` with zero state change. The fix replaced `if let Some` with `.ok_or_else(|| MutationError::Forbidden { reason: ForbiddenReason::PolicyAttachmentMissing })?`, making the apply helper self-defending regardless of whether validation ran.

## Lesson

> An apply function that silently no-ops when a required field is absent produces a successful mutation with no observable effect — always propagate an error so the caller knows the operation was a no-op

## Applies When

- An apply function requires a field to be present (e.g. `policy_attachment`, a parent record, a config entry)
- A separate validation layer guards against the absent case but is structurally decoupled from the apply layer
- The apply function could be called from multiple callers — not just the one that runs validation first

## Does Not Apply When

- The apply function is private and can only be reached through a single validated entry point (sealed by the type system)
- Idempotency is the intended behaviour: applying to an absent field should be a no-op by design

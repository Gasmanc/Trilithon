---
track: knowledge
problem_type: convention
title: "Never silently discard audit write failures with let _ ="
slug: no-silently-discarded-audit-writes
date: 2026-05-16
phase_id: "9"
generalizable: true
one_sentence_lesson: "Audit write failures must be logged at ERROR level, not silently discarded with 'let _ =' — silent discard makes audit gaps invisible until a compliance review"
tags: [audit, logging, error-handling, convention]
---

## Context

Phase 9 introduced audit logging across several handlers (change-password, adopt, reapply drift). In three independent handlers the audit write was called with `let _ = audit_writer.record(...).await` — copy-propagated from the first handler that used this pattern without comment.

## What Happened

The pattern was identified in the multi-reviewer aggregate as a recurring error pattern across the codebase. Silent discard means audit gaps (storage failures, serialisation errors) are invisible in production logs. The fix logs `AuditWriteError` at `ERROR` level via `?` propagation or an explicit `if let Err` block.

## Lesson

> Audit write failures must be logged at ERROR level, not silently discarded with 'let _ =' — silent discard makes audit gaps invisible until a compliance review

## Applies When

- Any call to `audit_writer.record(...)` or equivalent audit-append operation
- The handler has a `Result` return type that can propagate the error
- The audit log is used for security, compliance, or forensic purposes

## Does Not Apply When

- Best-effort telemetry (metrics, traces) where loss is explicitly acceptable
- A documented decision exists to not fail the request on audit errors (must still log)

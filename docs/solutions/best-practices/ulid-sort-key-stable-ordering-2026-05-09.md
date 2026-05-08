---
track: knowledge
problem_type: best-practice
title: "Sort append-only logs by ULID id column for stable ordering"
slug: ulid-sort-key-stable-ordering
date: 2026-05-09
phase_id: "6"
generalizable: true
one_sentence_lesson: "Sort append-only audit logs by ULID `id` column rather than the `occurred_at` timestamp to get stable ordering across rows with the same second — ULID embeds millisecond monotonic time so ties break correctly without a secondary sort key."
tags: [rust, sqlite, ulid, pagination, audit-log]
---

## Context

Phase 6 implemented `tail_audit_log` (slice 6.6) — a paginated query over `audit_log`. The initial spec suggested `ORDER BY occurred_at DESC`, which is the natural "most recent first" ordering. During implementation, the decision was made to switch to `ORDER BY id DESC`.

## What Happened

`occurred_at` has second-level granularity. Under any workload that generates more than one audit event per second (e.g., bulk imports, test suites), rows with identical `occurred_at` values produce non-deterministic ordering in the result set. Pagination cursors based on these unstable positions skip or duplicate rows.

ULIDs embed 48 bits of millisecond timestamp in the most significant bits, making them lexicographically monotonically increasing for events within the same process (with the same system clock). Sorting by `id DESC` is equivalent to sorting by generation time with millisecond resolution — stable ties within a second are broken by the random 80 bits of ULID entropy.

The `cursor_before` pagination parameter also uses the ULID as the exclusive upper bound (`WHERE id < :cursor`), which is only correct if rows are sorted by the same ULID key.

## Lesson

> Sort append-only audit logs by ULID `id` column rather than the `occurred_at` timestamp to get stable ordering across rows with the same second — ULID embeds millisecond monotonic time so ties break correctly without a secondary sort key.

## Applies When

- Implementing paginated queries over any ULID-keyed append-only table
- Choosing between `ORDER BY created_at` and `ORDER BY id` when the id column is a ULID
- Implementing cursor-based pagination where the cursor is the row's ULID id

## Does Not Apply When

- The table uses auto-increment integer primary keys (sort by `id DESC` still works but for different reasons)
- The query requires ordering by a business-level timestamp that the ULID does not reflect (e.g., `scheduled_at` for a job queue)
- Rows are keyed by UUID v4 (random, not monotonic — do not sort by UUID v4)

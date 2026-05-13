---
track: bug
problem_type: correctness
root_cause: insufficient-validation
resolution_type: refactored
severity: high
title: "Defensive CREATE TABLE IF NOT EXISTS fallbacks that diverge from canonical schema cause silent corruption"
slug: migration-fallback-diverges-canonical-schema
date: 2026-05-13
phase_id: "6"
generalizable: true
one_sentence_lesson: "A migration's defensive 'CREATE TABLE IF NOT EXISTS' fallback that diverges from the canonical schema is more dangerous than a missing table — remove the fallback entirely so any missing prerequisite fails loudly at migration time, not silently at first INSERT."
tags: [rust, sqlite, migrations, schema, audit]
---

## Context

Migration `0006_audit_immutable.sql` added immutability triggers to the `audit_log` table. It also contained a defensive `CREATE TABLE IF NOT EXISTS audit_log (...)` block intended to make the migration self-contained. However, that fallback schema omitted two columns added in `0001` — `prev_hash` and `caddy_instance_id` — that are required by `record_audit_event` at runtime.

## What Happened

If `0001` was never applied (e.g. a partial install, a fresh test environment, or a migration applied out of order), the fallback would silently create the table with the wrong shape. The triggers from `0006` would apply, but the first `INSERT` via `record_audit_event` would fail at runtime with a column-not-found error rather than at migration time. The defensive block was intended to prevent failure but instead deferred it to a harder-to-diagnose point. Removing it entirely causes the migration to fail loudly with "no such table: audit_log" if `0001` was not applied — which is the correct behaviour.

## Lesson

> A migration's defensive 'CREATE TABLE IF NOT EXISTS' fallback that diverges from the canonical schema is more dangerous than a missing table — remove the fallback entirely so any missing prerequisite fails loudly at migration time, not silently at first INSERT.

## Applies When

- A migration adds triggers, indices, or constraints to a table created by an earlier migration
- You are tempted to add `CREATE TABLE IF NOT EXISTS` so the migration appears self-contained
- The table schema has evolved since the original creation migration (any new columns, constraints, or defaults)

## Does Not Apply When

- The migration is itself the canonical table-creation migration (it should use `CREATE TABLE IF NOT EXISTS` to be idempotent)
- The fallback schema is provably byte-identical to the canonical schema and is maintained alongside it

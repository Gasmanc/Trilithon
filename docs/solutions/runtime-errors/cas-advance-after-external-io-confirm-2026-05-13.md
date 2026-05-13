---
track: bug
problem_type: race-condition
root_cause: concurrency-gap
resolution_type: refactored
severity: critical
title: "CAS pointer must advance only after external system confirms success"
slug: cas-advance-after-external-io-confirm
date: 2026-05-13
phase_id: "7"
generalizable: true
one_sentence_lesson: "When a DB pointer must stay consistent with an external system (Caddy), advance the pointer only after the external system confirms success — not optimistically before I/O"
tags: [rust, cas, sqlite, caddy, apply-path]
---

## Context

The apply path writes a new Caddy config to the admin API, then advances `applied_config_version` in SQLite so future callers know which snapshot is live. The CAS advance was originally placed at Step 0 (before sending the config to Caddy) as an optimistic lock claim, with a rollback if Caddy rejected the config.

## What Happened

Because the advisory lock serializes all concurrent `apply()` calls, the CAS advance at Step 0 was safe from races — but it meant `applied_config_version` could point to a version Caddy had never actually loaded. If the process crashed or was killed between Step 0 and the Caddy PUT, the version pointer was permanently ahead of the live Caddy state. The fix moved `cas_advance_config_version` to after `verify_equivalence` confirms Caddy accepted and loaded the new config.

## Lesson

> When a DB pointer must stay consistent with an external system (Caddy), advance the pointer only after the external system confirms success — not optimistically before I/O

## Applies When

- A DB field tracks which state an external system is currently running (config version, migration version, live snapshot ID)
- The write to the external system can fail, timeout, or be rejected after the DB pointer is already advanced
- A crash between the DB write and the external write would leave the DB in a state inconsistent with the external system

## Does Not Apply When

- The DB pointer is purely internal and the external system is always authoritative (the pointer is derived from external state, not the other way around)
- An explicit two-phase commit or saga pattern already handles rollback of the external write

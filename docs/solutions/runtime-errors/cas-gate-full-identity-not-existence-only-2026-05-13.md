---
track: bug
problem_type: correctness
root_cause: concurrency-gap
resolution_type: guard-added
severity: high
title: "CAS gate must assert full identity (ID + version), not just existence"
slug: cas-gate-full-identity-not-existence-only
date: 2026-05-13
phase_id: "7"
generalizable: true
one_sentence_lesson: "Existence-only queries in CAS gates are insufficient — always assert the full identity (ID + version) of the new state to prevent phantom version advances from ID collisions"
tags: [rust, cas, sqlite, snapshots, identity]
---

## Context

`advance_config_version_if_eq` in the snapshots adapter issues a CAS that checks whether a snapshot exists before advancing the `applied_config_version` pointer. The original query checked only `WHERE id = ? AND caddy_instance_id = ?` — existence of the snapshot row by ID.

## What Happened

A snapshot row is uniquely identified by both its ID *and* the `config_version` it was assigned at creation. An existence-only check allows a snapshot from a *different* config version (same ID, different version) to satisfy the guard — for instance if a snapshot ID was reused or if the version counter was reset. The fix extended the query to also check `AND config_version = ?` (the expected new version), so the gate only passes when the exact (id, version) pair exists.

## Lesson

> Existence-only queries in CAS gates are insufficient — always assert the full identity (ID + version) of the new state to prevent phantom version advances from ID collisions

## Applies When

- A CAS gate checks that a target entity exists before committing a state pointer advance
- The target entity has multiple identity dimensions (ID + version, content hash + timestamp, etc.)
- An ID collision or version mismatch could produce a false-positive existence check

## Does Not Apply When

- The entity's identity is a content-addressed hash that already encodes all relevant dimensions
- The existence check is a soft precondition (advisory) rather than a hard CAS gate

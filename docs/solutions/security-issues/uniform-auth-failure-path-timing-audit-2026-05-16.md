---
track: bug
problem_type: security
root_cause: missing-nil-check
resolution_type: guard-added
severity: high
title: "All auth failure paths must be uniform in time and audit actor"
slug: uniform-auth-failure-path-timing-audit
date: 2026-05-16
phase_id: "9"
generalizable: true
one_sentence_lesson: "All failed authentication paths must use the same audit actor (System, not User) and take the same wall-clock time to prevent username enumeration via timing or log inspection"
tags: [security, auth, timing, audit, enumeration]
---

## Context

The login handler short-circuited on a missing username without running Argon2 verification, making the missing-username path measurably faster than the wrong-password path. Additionally, the wrong-password path recorded `ActorRef::User { id: username }` in the audit log, while the missing-username path recorded `ActorRef::System` — letting audit log readers enumerate valid usernames.

## What Happened

Two fixes were combined: (1) `dummy_verify(&req.password)` is called when the username lookup returns nothing, running a full Argon2id verification against a pre-computed dummy hash to equalise timing. (2) Both failed-login branches now record `ActorRef::System { component: "auth" }` regardless of whether the username was found — audit logs cannot be used to distinguish the two cases.

## Lesson

> All failed authentication paths must use the same audit actor (System, not User) and take the same wall-clock time to prevent username enumeration via timing or log inspection

## Applies When

- A login handler has separate code paths for "user not found" vs. "wrong password"
- Audit logs record the actor identity (user ID, username) for failed events
- Response time is measurable by the caller (true for any network service)

## Does Not Apply When

- The system intentionally distinguishes account existence (e.g. account recovery flows with explicit disclosure)
- The auth system already runs all paths through the same crypto primitive

---
track: knowledge
problem_type: best-practice
title: "Return 501 when correct semantics aren't implemented yet"
slug: return-501-before-implementing-semantics
date: 2026-05-16
phase_id: "9"
generalizable: true
one_sentence_lesson: "When an endpoint's semantics cannot be correctly implemented yet, return 501 rather than silently falling back to a different (wrong) operation"
tags: [http, api, stubs, correctness]
---

## Context

The drift `adopt` endpoint was supposed to capture the running Caddy config and persist it as the new desired state. The applier trait did not yet expose `get_running_config`, so the handler fell back to loading `latest_desired_state()` and re-applying it — semantically identical to `reapply`.

## What Happened

An operator choosing "adopt" to accept a manually-changed Caddy config would silently get a reapply instead, overwriting the diverged state they intended to keep. The fix returned `Err(ApiError::NotImplemented("adopt-not-implemented"))` immediately, making the limitation explicit until `get_running_config` is wired.

## Lesson

> When an endpoint's semantics cannot be correctly implemented yet, return 501 rather than silently falling back to a different (wrong) operation

## Applies When

- An endpoint has a well-defined semantic contract that requires infrastructure not yet wired (external API calls, new trait methods, background state)
- A partial implementation would execute a different operation under the same endpoint name
- Silent wrong-operation is worse than a clear "not yet available" response

## Does Not Apply When

- A degraded-but-correct fallback exists (e.g. returning cached data when live data is unavailable)
- The endpoint is purely additive and safe to no-op

---
track: bug
problem_type: correctness
root_cause: wrong-type-assumption
resolution_type: algorithm-replaced
severity: critical
title: "Render DesiredState to Caddy JSON before diffing against live config"
slug: caddy-diff-render-before-compare
date: 2026-05-13
phase_id: "8"
generalizable: true
one_sentence_lesson: "When diffing live Caddy state against desired state, always compare same-schema JSON blobs — rendering DesiredState to Caddy JSON before comparing prevents false drift from schema mismatches"
tags: [caddy, drift-detection, json, schema, diff]
---

## Context

The `DriftDetector` periodically fetches the live Caddy config via `GET /config/`
and compares it against the stored `DesiredState`. The live Caddy response is
raw Caddy JSON (`{"apps": {"http": {...}}}`), while `DesiredState` uses
Trilithon's internal schema (`{routes, upstreams, policies}`). These schemas
are fundamentally incompatible.

## What Happened

The original implementation deserialised the raw Caddy JSON into a `DesiredState`
struct. Because `serde` silently ignores unknown fields, this produced a
`DesiredState` with all meaningful fields at their zero/default values. Diffing
this empty struct against the actual desired state caused every tick to report
drift — a constant false positive — even when Caddy and the stored config were
identical. The bug was invisible in unit tests because they bypassed the real
Caddy JSON format.

The fix: call `CaddyJsonRenderer::render(&desired)` to convert `DesiredState` →
Caddy JSON first, then use `diff_caddy_values()` to compare two Caddy JSON
`serde_json::Value` blobs. Both sides are now in the same schema.

## Lesson

> When diffing live Caddy state against desired state, always compare same-schema JSON blobs — rendering DesiredState to Caddy JSON before comparing prevents false drift from schema mismatches

## Applies When

- Comparing any two representations of configuration that live at different
  abstraction layers (internal model vs. external API format)
- Writing a drift detector or reconciler that fetches live state from an
  external system and compares it to an internal model
- Adding tests for a diff path — always verify the fake "clean state" uses
  the same JSON shape as the real system would return

## Does Not Apply When

- Both sides of a diff are already in the same schema (e.g. two `DesiredState`
  objects from storage — use `DefaultDiffEngine` for those)
- The external system's schema is identical to the internal model's serialised form

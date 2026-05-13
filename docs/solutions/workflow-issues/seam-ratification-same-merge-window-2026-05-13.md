---
track: knowledge
problem_type: workflow
title: "Cross-cutting phases must ratify seams in the same merge window"
slug: seam-ratification-same-merge-window
date: 2026-05-13
phase_id: "7"
generalizable: true
one_sentence_lesson: "Cross-cutting phases must register their architectural seams (and stub tests) in the same merge window — catch-up seam ratification compounds coherence debt exponentially with each subsequent phase"
tags: [cross-phase, seams, architecture, workflow]
---

## Context

This project uses a Foundation-2 cross-phase coherence layer: every phase that crosses architectural boundaries must register its seams in `docs/architecture/seams.md` and add stub integration tests in `core/crates/adapters/tests/cross_phase/`. Phase 7 implemented the full apply path (CAS, advisory locks, Caddy admin integration) but did not ratify its seams at merge time.

## What Happened

By the time Phase 7's review cycle ran, five unregistered seams had been identified: `applier-caddy-admin`, `applier-audit-writer`, `snapshots-config-version-cas`, `apply-lock-coordination`, and `apply-audit-notes-format`. Each seam that goes unregistered means subsequent phases cannot safely assume the boundary's contract — or worse, they silently consume it. Catch-up registration requires reconstructing intent from code that has since been modified, and each additional phase widens the gap.

## Lesson

> Cross-cutting phases must register their architectural seams (and stub tests) in the same merge window — catch-up seam ratification compounds coherence debt exponentially with each subsequent phase

## Applies When

- A phase introduces a new cross-layer or cross-service boundary (adapters calling core, core calling storage traits, any I/O boundary)
- The project has a Foundation-2 seam registry (`docs/architecture/seams.md`) and cross-phase integration tests
- Subsequent phases will build on the new boundary before the next review cycle

## Does Not Apply When

- The phase is purely internal to one layer and introduces no new external boundaries
- The seam already exists and is being modified (update existing entry, do not add a duplicate)

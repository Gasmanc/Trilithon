---
track: knowledge
problem_type: testing-pattern
title: "Schema drift tests must run the generator and check git diff"
slug: schema-drift-test-git-diff-check
date: 2026-05-06
phase_id: "4"
generalizable: true
one_sentence_lesson: "Schema drift tests must run the generator and check git diff — gating only on manual just check misses incremental CI builds"
tags: [rust, schemas, ci, testing, git, drift-detection]
---

## Context

Phase 4 introduced a schema generator binary (`gen_mutation_schemas`) that writes JSON Schema files under `docs/schemas/mutations/`. The `just check` gate ran the generator, but no integration test existed to detect drift in incremental CI builds that bypass `just check`.

## What Happened

The phase TODO (Slice 4.10) required a `schema_drift.rs` integration test, but only `mutation_props.rs` was present. Without the test, a developer could change a model type, run `cargo test`, see it pass, and push — schema files would drift from the code without any CI diagnostic. The fix created `crates/core/tests/schema_drift.rs` with a `schemas_match_committed` test that runs `gen_mutation_schemas` via `std::process::Command` and asserts `git diff --exit-code docs/schemas/mutations/` is clean.

## Lesson

> Schema drift tests must run the generator and check git diff — gating only on manual just check misses incremental CI builds

## Applies When

- A build step generates files that are committed to the repo (schemas, bindings, proto outputs, snapshots)
- The project has both a `just` gate and `cargo test` as CI steps — developers may run only the latter
- Generated files are referenced by downstream consumers and must stay in sync with source code

## Does Not Apply When

- Generated files are gitignored and always regenerated during build (no committed artefacts to drift)
- The project has a single CI step that always runs the full `just check` gate with no shortcircuit path

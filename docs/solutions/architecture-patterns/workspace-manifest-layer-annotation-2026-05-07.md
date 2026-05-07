---
track: knowledge
problem_type: architecture-pattern
title: "Annotate workspace deps by architectural tier to enforce layer boundaries"
slug: workspace-manifest-layer-annotation
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "Keep test-only crates out of [workspace.dependencies] and annotate workspace deps by architectural tier so manifest review alone can enforce the three-layer boundary."
tags: [rust, workspace, cargo, architecture, layering]
---

## Context

A three-layer Rust workspace (`core` → `adapters` → `cli`) enforces layer separation by manifest dependencies. When the workspace-level `Cargo.toml` accumulated I/O, async-runtime, and test-helper crates alongside pure-logic ones, manifest review could no longer catch violations of the architecture — any crate could accidentally import a forbidden dep without raising a red flag.

## What Happened

`core/Cargo.toml` declared `tokio`, `tracing-subscriber`, `nix`, `time`, `assert_cmd`, `predicates`, `insta`, and `regex` as workspace dependencies alongside `serde` and `thiserror`. This meant the workspace manifest provided no architectural signal — `core` layer code could `use tokio::...` without any build error. The fix stripped the workspace manifest to pure-logic crates only and moved I/O/async/test deps to per-crate manifests, with inline `# layer: adapters | cli | dev-only` comments on each line.

## Lesson

> Keep test-only crates out of [workspace.dependencies] and annotate workspace deps by architectural tier so manifest review alone can enforce the three-layer boundary.

## Applies When

- Building a multi-crate workspace with enforced layer separation (`core`/`adapters`/`cli` or similar)
- Adding any new workspace-level dependency — ask which layer(s) actually need it
- Reviewing a PR that touches `Cargo.toml` — the absence of a tier annotation is a smell

## Does Not Apply When

- Single-crate projects (no layering to enforce)
- Monorepos where all crates are peers with equal privilege

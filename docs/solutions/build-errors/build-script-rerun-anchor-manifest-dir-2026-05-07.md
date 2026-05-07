---
track: bug
problem_type: build
root_cause: environment-mismatch
resolution_type: config-fixed
severity: high
title: "Anchor build script rerun-if-changed paths to CARGO_MANIFEST_DIR"
slug: build-script-rerun-anchor-manifest-dir
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "Build script cargo:rerun-if-changed paths must be absolute or anchored to CARGO_MANIFEST_DIR — bare relative paths resolve to the package directory, not the workspace or git root."
tags: [rust, cargo, build-script, rerun-if-changed, git]
---

## Context

A CLI crate's `build.rs` embeds `TRILITHON_GIT_SHORT_HASH` by reading `git rev-parse HEAD`. The `cargo:rerun-if-changed=.git/HEAD` directive was supposed to re-run the build script whenever the git HEAD changed (i.e., after every commit).

## What Happened

`cargo:rerun-if-changed=.git/HEAD` is interpreted relative to the package directory, which is `core/crates/cli/`. The path resolves to `core/crates/cli/.git/HEAD`, which does not exist. Cargo treats a missing `rerun-if-changed` target as "never triggers", so the build script only ran once (on first build). After subsequent commits, `TRILITHON_GIT_SHORT_HASH` remained stale. The fix used `PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.git/HEAD")` to get an absolute path pointing to the workspace git root.

## Lesson

> Build script cargo:rerun-if-changed paths must be absolute or anchored to CARGO_MANIFEST_DIR — bare relative paths resolve to the package directory, not the workspace or git root.

## Applies When

- Any `build.rs` that watches git state (`.git/HEAD`, `.git/refs/`)
- Any `build.rs` that watches files outside the package directory
- Embedding version info, build metadata, or schema hashes in binaries

## Does Not Apply When

- The watched file is inside the package directory (relative path is correct)
- Using `cargo:rerun-if-env-changed` (env vars, not file paths)

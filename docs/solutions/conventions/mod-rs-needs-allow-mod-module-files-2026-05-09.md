---
track: knowledge
problem_type: convention
title: "Every mod.rs file in this workspace needs the mod_module_files allow attribute"
slug: mod-rs-needs-allow-mod-module-files
date: 2026-05-09
phase_id: "6"
generalizable: true
one_sentence_lesson: "Every `mod.rs` file in this workspace must contain `#![allow(clippy::mod_module_files)]` because the global clippy config prohibits the `mod.rs` pattern by default."
tags: [rust, clippy, workspace-convention, module-structure]
---

## Context

Phase 6 created three new `mod.rs` files (`crates/core/src/audit/mod.rs`, `crates/core/src/schema/mod.rs`, `crates/adapters/src/storage_sqlite/mod.rs`). The gate failed immediately on each with `error: module in file found instead of inline module` from the `mod_module_files` lint.

## What Happened

The workspace enables `clippy::mod_module_files` globally, which prefers `foo.rs` over `foo/mod.rs` for module files. However, the existing codebase uses `mod.rs` throughout (e.g., `mutation/mod.rs`, `storage/mod.rs`), so the pattern is established. The lint fires on new files but is suppressed by `#![allow]` in the existing ones. Each new `mod.rs` must carry its own suppression; the global config is intentional (it catches accidental new module files) but the workspace has accepted `mod.rs` as the pattern for submodule roots.

Add at the top of every new `mod.rs`:

```rust
#![allow(clippy::mod_module_files)]
// reason: workspace enforces this lint globally; mod.rs is the established
// pattern for submodule roots in this codebase
```

This recurred in slices 6.1, 6.3, and 6.4.

## Lesson

> Every `mod.rs` file in this workspace must contain `#![allow(clippy::mod_module_files)]` because the global clippy config prohibits the `mod.rs` pattern by default.

## Applies When

- Creating a new `foo/mod.rs` submodule root under any crate in this workspace
- The clippy gate fires with `module in file found instead of inline module`

## Does Not Apply When

- Creating a flat `foo.rs` module (no `mod.rs` needed — this lint does not fire)
- Projects that don't enforce `mod_module_files` globally

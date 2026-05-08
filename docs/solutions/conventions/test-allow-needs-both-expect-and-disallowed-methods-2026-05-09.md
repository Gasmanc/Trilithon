---
track: knowledge
problem_type: convention
title: "Test allow lists need both clippy::expect_used and clippy::disallowed_methods"
slug: test-allow-needs-both-expect-and-disallowed-methods
date: 2026-05-09
phase_id: "6"
generalizable: true
one_sentence_lesson: "In this workspace, test `#![allow]` blocks need both `clippy::expect_used` AND `clippy::disallowed_methods` — the two lints independently block `.expect()`, so allowing only `expect_used` is insufficient."
tags: [rust, clippy, testing, workspace-convention]
---

## Context

Phase 6 introduced multiple integration test files across slices 6.1, 6.2, 6.4, 6.5, and 6.6. Each test file needed `.expect()` and `.unwrap()` calls — standard for test code that should panic on failure. Every slice hit the same clippy gate failure despite adding `#![allow(clippy::expect_used)]`.

## What Happened

The workspace's clippy config enforces `clippy::disallowed_methods` globally as a separate lint from `clippy::expect_used`. These two lints overlap: `.expect()` is independently blocked by both. Adding only `clippy::expect_used` silences one lint but the other continues to fire. The fix was to add both to the allow list alongside `clippy::unwrap_used`:

```rust
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
)]
// reason: test-only code; panics are the correct failure mode in tests
```

This pattern recurred in 5 of 7 Phase 6 slices before being standardised.

## Lesson

> In this workspace, test `#![allow]` blocks need both `clippy::expect_used` AND `clippy::disallowed_methods` — the two lints independently block `.expect()`, so allowing only `expect_used` is insufficient.

## Applies When

- Writing any new integration test file under `crates/*/tests/`
- Adding `#[cfg(test)] mod tests` inline modules that use `.expect()` or `.unwrap()`
- Any test file that also uses `panic!` (add `clippy::panic` too)

## Does Not Apply When

- Production source files — neither lint should be suppressed there
- Projects that don't enforce `disallowed_methods` globally

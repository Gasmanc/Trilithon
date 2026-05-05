---
name: Never use unreachable!() in production match arms over validated inputs
description: unreachable!() panics the process; any match arm that could be reached under concurrent writes or future enum additions must return a proper error instead
type: solution
category: conventions
phase_id: onboard-review-doc
source_commit: cf425a4
source_date: 2026-05-03
one_sentence_lesson: Replace unreachable!() in production match arms with a proper error return — concurrent writes or future enum additions can reach arms that look impossible at review time, and a panic is worse than a handled error.
---

## Problem

`apply_variant` in `apply.rs` had an arm for `Mutation::Rollback` that used `unreachable!()`:

```rust
Mutation::Rollback { .. } => unreachable!("rollback is handled before apply_variant"),
```

The comment claimed this arm was unreachable because the caller filtered it out. But:
1. Under concurrent mutations the invariant could be violated
2. If a new code path called `apply_variant` without the pre-filter, the process would panic with no recovery path
3. `unreachable!()` is indistinguishable from a real logic bug to an operator reading a crash report

## Fix

Return a structured error:

```rust
Mutation::Rollback { .. } => Err(MutationError::Forbidden {
    reason: "Rollback must be resolved before apply_variant is called".into(),
}),
```

## Rule

In production code (non-test), never use `unreachable!()`, `panic!()`, or `todo!()` in:
- Match arms over enums that arrive from external or validated input
- Code paths whose "impossibility" depends on a caller contract rather than the type system

Use `unreachable!()` only when the type system already makes the case impossible (e.g., matching on an infallible `!` type) or in code that is exclusively compiled under `#[cfg(test)]`.

## Category

conventions

---
name: Three-state PATCH semantics with double_option
description: When designing PATCH/update operations, plain Option<T> cannot distinguish "clear the field" from "don't touch it" — use a three-state wrapper type
type: solution
category: architecture-patterns
phase_id: onboard-git-history
source_commit: cf425a4
source_date: 2026-05-03
one_sentence_lesson: Plain Option<T> in a PATCH struct cannot distinguish "set to null/clear" from "field absent/unchanged" — use a three-state type like double_option so callers can explicitly clear optional fields.
---

## Problem

`RoutePatch` and `UpstreamPatch` structs used `Option<T>` for nullable fields like `redirects`, `policy_attachment`, and `max_request_bytes`. This meant:

- `None` = "don't touch this field" (intended)
- `Some(None)` = "clear this field" (intended but indistinguishable from above when deserialised from JSON `null`)

A JSON PATCH body of `{"redirects": null}` and a body omitting `redirects` entirely both deserialised to `None`, so callers couldn't explicitly clear an optional field.

## Fix

Use `double_option::deserialize` (or equivalent) to deserialise into `Option<Option<T>>`:

- `None` (field absent in JSON) → don't touch
- `Some(None)` (field present as `null`) → clear
- `Some(Some(v))` (field present with value) → set to `v`

```rust
#[serde(
    default,
    skip_serializing_if = "Option::is_none",
    deserialize_with = "double_option::deserialize"
)]
pub redirects: Option<Option<RedirectRules>>,
```

## When to apply

Any time a struct models a partial update (PATCH semantics) and the domain allows clearing an optional field back to "none". Required when the field is an `Option<T>` in the target struct.

## Category

architecture-patterns

---
track: knowledge
problem_type: architecture-pattern
title: "Parse and validate network types in the adapters layer, not core"
slug: io-adjacent-crates-belong-in-adapters
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "In a three-layer architecture, I/O-adjacent crates (even pure parsers like 'url') belong in the adapters layer; represent external addresses as String in core types and parse them at the adapters boundary."
tags: [rust, architecture, layering, core, adapters, url, parsing]
---

## Context

A three-layer architecture enforces that `core` contains only pure logic with no I/O, network, or FFI dependencies. A `CaddyEndpoint::LoopbackTls` variant in `core` needed to carry a base URL, which led to importing `url = { workspace = true }` into the `core` crate manifest.

## What Happened

`url::Url` was used as a field type in a `core` enum variant. While `url` is technically a "pure parser" (no network I/O), it is network-adjacent and adds cognitive coupling to network concepts in the pure-logic layer. The architectural rule is enforced by the *absence* of certain imports in manifests; importing `url` broke that signal. The fix replaced `Url` with `String` in the `core` type, moved URL validation to the `adapters` layer (where `hyper` already depended on `url`), and added a `validate_endpoint()` function in `adapters`.

## Lesson

> In a three-layer architecture, I/O-adjacent crates (even pure parsers like 'url') belong in the adapters layer; represent external addresses as String in core types and parse them at the adapters boundary.

## Applies When

- Deciding where to parse or validate network addresses, file paths, or external identifiers in a layered codebase
- Adding any crate to `core/` that deals with external system concepts (URLs, socket addresses, MIME types)
- Reviewing a `core/` manifest change that adds a non-`serde`/`thiserror` dependency

## Does Not Apply When

- The "core" layer has been explicitly expanded in scope (e.g., the project has accepted certain parsing deps in core)
- The crate is genuinely pure-logic (no domain-external concepts) and has no I/O behavior

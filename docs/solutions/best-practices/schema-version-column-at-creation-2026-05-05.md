---
track: knowledge
problem_type: best-practice
root_cause: missing-field
resolution_type: schema-change
severity: high
title: "Add a DB column for schema-version markers at model creation time"
slug: schema-version-column-at-creation
date: 2026-05-05
phase_id: "5"
generalizable: true
one_sentence_lesson: "When a Rust model field carries a schema-version marker, add a DB column for it immediately rather than defaulting at read time — retrofitting after a format bump is much more expensive than adding the column upfront."
tags: [rust, sqlite, sqlx, schema-migration, versioning]
---

## Context

`Snapshot` carries a `canonical_json_version: u32` field that records which
version of the canonical-JSON serialiser produced the stored payload. This
field exists so that a future format change can be detected and the snapshot
re-derived rather than silently misread. During Phase 5 the field was present
in the Rust struct but absent from the SQLite schema — reads defaulted to `1`
at the application layer instead of being stored and retrieved from the DB.

## What Happened

When the absence was caught in review (F006), the fix required a new migration
(`0005_canonical_json_version.sql`), a schema change to the `snapshots` table,
updates to every `SELECT` that projects columns by position, a new `INSERT`
binding, and changes to `row_to_snapshot` to parse the new column. Migration
count tests (`migrations_parse.rs`, `migrate.rs`) also had to be updated. Had
the column been added in the same commit that introduced the struct field, the
diff would have been a single migration with a two-line `INSERT` change.

## Lesson

> When a Rust model field carries a schema-version marker, add a DB column for
> it immediately rather than defaulting at read time — retrofitting after a
> format bump is much more expensive than adding the column upfront.

## Applies When

- Adding a field to a persisted Rust struct that records a format, codec, or
  serialiser version (e.g. `encoding_version`, `schema_version`,
  `compression_codec`).
- The field has a sensible default today but **must** be queryable or
  filterable later (e.g. to trigger re-derivation jobs).
- You are writing a new migration for a related change — adding the column then
  is essentially free.

## Does Not Apply When

- The field is truly ephemeral and never needs to be stored (e.g. a
  computed display value derived at read time from other persisted columns).
- The project is still in a pre-schema phase where no migration tooling is wired
  up yet; in that case add the column when migrations are introduced.

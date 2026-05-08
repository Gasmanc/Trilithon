---
track: bug
problem_type: security
root_cause: missing-validation
resolution_type: fixed
severity: high
title: "Escape LIKE wildcards in user-controlled SQL query parameters"
slug: sqlite-like-wildcard-escape
date: 2026-05-08
phase_id: "onboard-git-history"
source_commit: 553dce5
generalizable: true
one_sentence_lesson: "LIKE queries built from user-controlled input must escape %, _, and \\ before appending the % suffix and must specify an ESCAPE clause — omitting this lets callers inject wildcards that match unintended rows."
tags: [rust, sqlite, sqlx, sql-injection, like, security, input-validation]
---

## Context

`tail_audit_log` accepted an optional glob prefix filter. The implementation extracted the prefix from the glob pattern and appended `%` to build a `LIKE` predicate: `WHERE correlation_id LIKE ?1 || '%'`. The raw prefix was bound directly as the parameter.

## What Happened

A caller supplying a prefix containing `%`, `_`, or `\` could inadvertently (or deliberately) widen the match. A prefix of `%` matches every row; a prefix of `abc%def` matches any `correlation_id` starting with `abc` followed by anything before `def`. In an audit log, over-broad matches leak records the caller was not supposed to see.

The fix escaped the three special LIKE characters before binding and added an explicit `ESCAPE '\'` clause:

```rust
fn escape_like(s: &str) -> String {
    s.replace('\\', r"\\").replace('%', r"\%").replace('_', r"\_")
}
// usage:
WHERE correlation_id LIKE ? || '%' ESCAPE '\'
// bound value: escape_like(prefix)
```

## Lesson

> LIKE queries built from user-controlled input must escape `%`, `_`, and `\` before appending the `%` suffix and must specify an `ESCAPE` clause — omitting this lets callers inject wildcards that match unintended rows.

## Applies When

- Building a `LIKE` predicate from a user-supplied string (glob prefix, search term, filter)
- Any SQL database: SQLite, PostgreSQL, MySQL all support `ESCAPE` in `LIKE`

## Does Not Apply When

- The LIKE pattern is a compile-time constant with no user-controlled content
- You are using a full-text-search extension (FTS5 `MATCH`) rather than `LIKE`

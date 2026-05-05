---
track: knowledge
problem_type: security-pattern
root_cause: insufficient-validation
resolution_type: refactored
severity: high
title: "Prefer static SQL with COALESCE / IS NULL patterns over format!()-built queries for optional filters"
slug: sqlite-static-sql-coalesce-optional-filters
date: 2026-05-05
phase_id: "5"
generalizable: true
one_sentence_lesson: "Replace format!()-constructed SQL strings (which open an injection surface) with a single static query using `? IS NULL OR col = ?` or `COALESCE(?, col) = col` — the query planner handles the optional filter correctly with no dynamic string building."
tags: [rust, sqlite, sqlx, sql-injection, security, static-sql]
---

## Context

`fetch_by_date_range` in `SqliteStorage` built its WHERE clause dynamically using `format!()`, appending ` AND created_at >= ?` or ` AND created_at <= ?` depending on which bound was `Some`. The same pattern appeared in two other fetch helpers.

## What Happened

`format!()`-constructed SQL strings are a SQL injection vector if any interpolated value ever comes from untrusted input — even if today's call sites are internal, the pattern establishes a precedent that is easy to copy incorrectly. Additionally, sqlx cannot validate the query at compile time when the SQL is a runtime string.

The fix replaces dynamic construction with a single static SQL string that uses parameter binding for optional filters:

```sql
-- Static query handles optional bounds without string building:
SELECT ... FROM snapshots
WHERE (? IS NULL OR created_at >= ?)
  AND (? IS NULL OR created_at <= ?)
ORDER BY created_at ASC
```

Each optional bound is passed twice: once as the NULL sentinel check, once as the actual value. When the bound is `None`, both binds receive `None` and the filter is a no-op. When the bound is `Some(v)`, both receive `v` and the filter is applied.

## Lesson

> Replace format!()-constructed SQL strings (which open an injection surface) with a single static query using `? IS NULL OR col = ?` or `COALESCE(?, col) = col` — the query planner handles the optional filter correctly with no dynamic string building.

## Applies When

- Any query that conditionally includes WHERE clauses based on `Option<T>` arguments
- sqlx write paths where compile-time query checking (`query!` macro) is desired
- Code review: flag any `format!("SELECT ... WHERE {}", ...)` pattern in SQL-building code

## Does Not Apply When

- The number of optional filter dimensions is so large (10+) that a single static query becomes unreadable — in that case use a query builder with explicit parameterization (never string interpolation)
- The optional clause changes the JOIN structure, not just a WHERE predicate

---
name: Seed FK parent rows in a migration before code writes to the FK child table
description: If code writes to a FK child table before any user-setup has created the parent row, a fresh install fails with a FK constraint violation — seed the parent row in a migration
type: solution
category: best-practices
phase_id: onboard-git-history
source_commit: 8a5180d
source_date: 2026-05-03
one_sentence_lesson: Add a migration that seeds the canonical FK parent row so that code writing to the child table on a fresh install never hits a constraint violation before the user has done any setup.
---

## Problem

`capability_probe_results` has a FK on `caddy_instances(id)`. On a fresh install, the probe runner (which writes probe results) started before any `caddy_instances` row existed, causing an FK constraint failure on the first probe insert.

The FK parent row was only created by runtime code (`run.rs`) at startup, but the probe runner started in parallel and could race it.

## Fix

Add a migration that seeds the well-known "local" instance row unconditionally:

```sql
-- 0003_seed_local_instance.sql
INSERT OR IGNORE INTO caddy_instances
    (id, display_name, transport, address, created_at, ownership_token)
VALUES
    ('local', 'Local', 'unix', '', 0, '');
```

`INSERT OR IGNORE` makes this idempotent — re-running the migration on an existing DB with the row already present is a no-op.

## When to apply

Any time:
1. A table has a FK referencing another table's canonical/default row
2. Code writes to the child table before any user action would create the parent row
3. The parent row has a known constant value (like 'local', 'default', 'system')

Seed it in a migration so it's always present before the application starts.

## Category

best-practices

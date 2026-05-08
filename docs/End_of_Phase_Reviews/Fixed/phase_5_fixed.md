---
id: duplicate:area::phase-5-fixed-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-5-fixed-findings
  multi: false
finding_kind: legacy-uncategorized
phase_introduced: unknown
status: open
created_at: migration
created_by: legacy-migration
last_verified_at: 0a795583ea9c4266e7d9b0ae0f56fd47d2ecf574
severity: medium
do_not_autofix: false
---

# Phase 5 — Fixed Findings

**Run date:** 2026-05-05T19:54:00Z
**Total fixed:** 14

| ID | Severity | Title | File | Commit | PR | Date |
|----|----------|-------|------|--------|----|------|
| F001 | CRITICAL | Canonicalizer large integer f64 corruption | `core/crates/core/src/canonical_json.rs` | pre-review | — | 2026-05-05 |
| F002 | CRITICAL | Monotonicity check TOCTOU race | `core/crates/adapters/src/sqlite_storage.rs` | pre-review | — | 2026-05-05 |
| F004 | HIGH | InMemoryStorage diverges from SqliteStorage on duplicate semantics | `core/crates/core/src/storage/in_memory.rs` | pre-review | — | 2026-05-05 |
| F005 | HIGH | Deduplication path returns early inside open transaction | `core/crates/adapters/src/sqlite_storage.rs` | pre-review | — | 2026-05-05 |
| F006 | HIGH | `canonical_json_version` not persisted to database | `core/crates/adapters/src/sqlite_storage.rs`, `migrations/0005_canonical_json_version.sql` | `9c9fa93` | — | 2026-05-05 |
| F007 | HIGH | Snapshot content hash not verified in write path | `core/crates/adapters/src/sqlite_storage.rs` | pre-review | — | 2026-05-05 |
| F008 | WARNING | `created_at_monotonic_nanos` is a wall-clock value, not a monotonic counter | `core/crates/core/src/storage/types.rs` | `9c9fa93` | — | 2026-05-05 |
| F009 | WARNING | Dedup early-return bypasses config_version monotonicity check | `core/crates/adapters/src/sqlite_storage.rs` | `9c9fa93` | — | 2026-05-05 |
| F010 | WARNING | `fetch_by_date_range` with empty range performs unbounded full table scan | `core/crates/adapters/src/sqlite_storage.rs` | `9c9fa93` | — | 2026-05-05 |
| F011 | WARNING | `fetch_by_date_range` builds SQL with `format!` — fragile pattern | `core/crates/adapters/src/sqlite_storage.rs` | pre-review | — | 2026-05-05 |
| F014 | WARNING | `InMemoryStorage` ABBA lock ordering deadlock risk | `core/crates/core/src/storage/in_memory.rs` | `9c9fa93` | — | 2026-05-05 |
| F015 | WARNING | `Snapshot::intent` doc contradicts implementation; enforcement missing | `core/crates/core/src/storage/types.rs` | pre-review | — | 2026-05-05 |
| F016 | WARNING | Lint suppressions missing required `zd:` tracked-id format | `core/crates/adapters/src/sqlite_storage.rs` | pre-review | — | 2026-05-05 |
| F017 | WARNING | `fetch_by_parent_id` sort order inconsistent with other fetch methods | `core/crates/adapters/src/sqlite_storage.rs` | `9c9fa93` | — | 2026-05-05 |
| F018 | WARNING | Broken ADR link in `core/README.md` | `core/README.md` | pre-review | — | 2026-05-05 |
| F022 | SUGGESTION | `SnapshotId` accepts arbitrary strings without hex validation | `core/crates/core/src/storage/types.rs` | `9c9fa93` | — | 2026-05-05 |
| F023 | SUGGESTION | `let _ = parse_actor_kind(...)` pattern is unclear | `core/crates/adapters/src/sqlite_storage.rs` | pre-review | — | 2026-05-05 |
| F025 | SUGGESTION | `cast_sign_loss` suppression relies on implicit invariant | `core/crates/adapters/src/sqlite_storage.rs` | pre-review | — | 2026-05-05 |

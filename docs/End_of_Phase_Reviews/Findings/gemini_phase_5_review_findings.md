# Phase 5 — Gemini Review Findings

**Reviewer:** gemini
**Date:** 2026-05-05
**Diff range:** d4a320a..HEAD
**Phase:** 5

---

[CRITICAL] Canonicalizer Corrupts Large Integers
File: core/crates/core/src/canonical_json.rs
Lines: 65-83
Description: `canonicalise_value` converts all `Value::Number` variants to `f64` to normalize whole numbers. However, `f64` only represents integers exactly up to 2^53. For larger integers, `as_f64()` loses precision, and the subsequent `f as i64` cast produces an incorrect value. This corrupts the state before hashing, leading to incorrect content addresses.
Suggestion: Check `if n.is_f64()` before attempting normalization. Only convert to integer if the number is already a float and its value is within the safe integer range (-2^53 to 2^53). Leave `is_i64()` and `is_u64()` values untouched.

[HIGH] Missing Database Schema Updates
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 294-315, 336-410
Description: Migration 0004 adds triggers but fails to add the `canonical_json_version` and `created_at_monotonic_nanos` columns. The code repurposes `created_at_ms` for monotonic nanos and hardcodes the canonical version to 1, preventing future format detection.
Suggestion: Add a migration to include ALTER TABLE snapshots ADD COLUMN for the missing fields, and update SQL queries accordingly.

[WARNING] Low-Precision Monotonic Clock Implementation
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 308-311, 342-346
Description: The implementation maps `created_at_monotonic_nanos` (u64) to `created_at_ms` (i64) by dividing by 1,000,000, losing 6 digits of precision. Furthermore, populating this field from a wall-clock source violates the "monotonic" requirement.
Suggestion: Store high-precision nanoseconds in a dedicated INTEGER column and use a true monotonic clock source.

[SUGGESTION] Non-Atomic Transaction in Snapshot Insertion
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 348-350
Description: `self.pool.begin().await` starts a DEFERRED transaction. The initial SELECT for deduplication doesn't acquire a write lock, meaning concurrent writers could both see "no existing row" before one fails during INSERT.
Suggestion: Use BEGIN IMMEDIATE to ensure the write lock is acquired at the start of the transaction.

[SUGGESTION] Missing Instance Filtering in Version Fetch
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 96-121
Description: `fetch_by_config_version` fetches snapshots matching a version but does not filter by `caddy_instance_id`, which could return unrelated snapshots in multi-instance deployments.
Suggestion: Add a `caddy_instance_id` parameter to `fetch_by_config_version`.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-05 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Canonicalizer Corrupts Large Integers | ✅ Fixed | pre-review | — | 2026-05-05 | is_f64() guard already in canonical_json.rs |
| 2 | Missing Database Schema Updates | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F006: migration 0005 adds canonical_json_version column |
| 3 | Low-Precision Monotonic Clock Implementation | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F008: doc comment corrected to reflect wall-clock ms basis |
| 4 | Non-Atomic Transaction in Snapshot Insertion | ✅ Fixed | pre-review | — | 2026-05-05 | BEGIN IMMEDIATE already in insert_snapshot_inner |
| 5 | Missing Instance Filtering in Version Fetch | 🚫 Won't Fix | — | — | — | V1 single-instance design; caddy_instance_id always 'local' per ADR-0009 |

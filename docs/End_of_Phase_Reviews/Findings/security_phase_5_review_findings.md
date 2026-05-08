---
id: security:area::phase-5-security-review-findings:legacy-uncategorized
category: security
kind: process
location:
  area: phase-5-security-review-findings
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

# Phase 5 — Security Review Findings

**Reviewer:** security
**Date:** 2026-05-05
**Diff range:** d4a320a..HEAD
**Phase:** 5

---

[WARNING] INTENT FIELD BOUND IS ADVISORY, NOT ENFORCED AT WRITE PATH
File: core/crates/core/src/storage/types.rs
Lines: 62-98
Description: `Snapshot::intent` is a public `String` field. The doc comment says "Constructors MUST enforce this limit" and refers to `Snapshot::new`, but no such constructor exists. `validate_intent` is `#[must_use]` but there is no call site in the production write path (`insert_snapshot`). An oversized intent (up to SQLite's 1 GB text limit) can be stored verbatim.
Category: Input validation
Attack vector: A caller constructs a Snapshot directly with a multi-megabyte intent string; `insert_snapshot` stores it without checking the bound.
Suggestion: Either make `intent` private with a fallible constructor, or add an explicit check at the top of `insert_snapshot` returning StorageError when `!Snapshot::validate_intent(&snapshot.intent)`.

[WARNING] caddy_instance_id HARDCODED TO 'local' — MONOTONICITY BYPASS POSSIBLE
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 389, 411
Description: The monotonicity check filters on `caddy_instance_id = 'local'` as a raw string literal, not using the snapshot's own instance identifier. If a caller passes a Snapshot for a different instance, the monotonicity check consults the wrong partition and the INSERT silently stores the wrong value.
Category: Input validation / unsafe data handling
Attack vector: Supply a Snapshot whose config_version is lower than any existing local row but whose intended instance is different; the monotonicity check passes because it only reads the local partition.
Suggestion: Remove the hardcoded string. Derive caddy_instance_id from a validated constant or storage instance configuration.

[WARNING] fetch_by_date_range CONSTRUCTS SQL WITH format! — STRUCTURALLY FRAGILE
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 178-205
Description: The where_clause is assembled from static string literals, so no injection today. However, the pattern `format!(r"... {where_clause} ...")` directly embeds a runtime string into SQL. If SnapshotDateRange gains a user-controlled sort column, it becomes a SQL injection vector.
Category: Injection vectors
Attack vector: Future maintenance adds a user-controlled sort column to SnapshotDateRange; it gets interpolated without binding.
Suggestion: Refactor to select between four fully static query strings (one per combination of since/until present/absent) rather than building SQL with format!.

[SUGGESTION] snapshot_id IS NOT VALIDATED AS 64-CHARACTER LOWERCASE HEX BEFORE STORAGE
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 339, 354-369
Description: `insert_snapshot` uses `snapshot.snapshot_id.0` directly as the primary key without verifying it is a 64-character lowercase hex string. A caller supplying an arbitrary string as the ID could trigger a misleading SnapshotHashCollision error path.
Category: Input validation
Suggestion: Add a format check at the top of `insert_snapshot`: verify `id.len() == 64` and all characters are lowercase hex digits.

[SUGGESTION] actor_kind IS SILENTLY DISCARDED ON READ; STORED AS FIXED "system" ON WRITE
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 305-306, 343
Description: On write, actor_kind is hardcoded to "system". On read, actor_kind is parsed (to validate) then discarded. The audit trail cannot distinguish user-initiated from system-initiated snapshots, and any existing rows with actor_kind = 'user' or 'token' have that information permanently ignored.
Category: Sensitive data handling
Suggestion: Either preserve actor_kind in the Snapshot struct or document the intentional decision with a tracked issue reference.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-05 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | INTENT FIELD BOUND IS ADVISORY, NOT ENFORCED AT WRITE PATH | ✅ Fixed | pre-review | — | 2026-05-05 | validate_snapshot_invariants calls validate_intent before touching DB |
| 2 | caddy_instance_id HARDCODED TO 'local' — MONOTONICITY BYPASS POSSIBLE | 🚫 Won't Fix | — | — | — | V1 single-instance design; documented inline with ADR-0009 references |
| 3 | fetch_by_date_range CONSTRUCTS SQL WITH format! — STRUCTURALLY FRAGILE | ✅ Fixed | pre-review | — | 2026-05-05 | Four static query strings already in use |
| 4 | snapshot_id IS NOT VALIDATED AS 64-CHARACTER LOWERCASE HEX BEFORE STORAGE | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F022: SnapshotId::try_from_hex added; validate_snapshot_invariants also verifies at write path |
| 5 | actor_kind IS SILENTLY DISCARDED ON READ; STORED AS FIXED "system" ON WRITE | 🚫 Won't Fix | — | — | — | Intentional V1 design with inline comment |

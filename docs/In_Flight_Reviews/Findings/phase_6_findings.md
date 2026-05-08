---
id: duplicate:area::phase-6-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-6-findings
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

## Slice 6.1
**Status:** complete
**Date:** 2026-05-08
**Summary:** Restructured `audit.rs` into `audit/mod.rs` + `audit/event.rs`. Added `Hash`, `#[non_exhaustive]`, `kind_str()` const fn, `FromStr`, `AuditEventParseError`, and `AUDIT_KIND_REGEX`. All six acceptance tests pass. The `just check-rust` gate passes; the overall `just check` gate has a pre-existing prettier failure in the web frontend unrelated to this slice.

### Simplify Findings
- **Efficiency (test):** `no_two_variants_share_a_kind` was allocating `String` for each variant via `to_string()` and building a `HashSet<String>`. Changed to use `kind_str()` and `HashSet<&str>` to avoid heap allocation in test code.

### Items Fixed Inline
- `no_two_variants_share_a_kind` — used `to_string()` / `HashSet<String>` unnecessarily; replaced with `kind_str()` / `HashSet<&str>` (efficiency, test code, `core/crates/core/src/audit/event.rs` line 465)

### Items Left Unfixed
none

## Slice 6.2
**Status:** complete
**Date:** 2026-05-08
**Summary:** Defined `AuditRowId`, `ActorRef`, `AuditOutcome`, `AuditEventRow`, `AuditSelector`, and `NormalisedAuditSelector` in `core/crates/core/src/audit/row.rs`. Added `Serialize`/`Deserialize` impls to `AuditEvent` in `event.rs` (delegating to `kind_str()`/`from_str()`). All four acceptance tests pass; `just check` Rust gate passes cleanly (pre-existing web/Prettier failures are unrelated to this slice).

### Simplify Findings
- **Quality:** Redundant test-module imports (`use crate::audit::event::AuditEvent` and `use crate::storage::types::SnapshotId`) — already in scope via `use super::*`. Removed.
- **Quality:** Misleading "Externally tagged" comment in `actor_serialises_externally_tagged` test — the attribute is `#[serde(tag = "kind")]` (internally tagged). Comment corrected.

### Items Fixed Inline
- Redundant test imports removed — `row.rs` tests (quality)
- Corrected misleading "externally tagged" → "internally tagged" comment — `row.rs` tests (quality)

### Items Left Unfixed
none

## Slice 6.5
**Status:** complete
**Date:** 2026-05-08
**Summary:** Implemented `AuditWriter::record` as the sole entry point to `audit_log`. Added the `Clock` trait and `SystemClock` to `core`, published `pub mod redactor` and `pub mod schema` from core's lib.rs, and wired `AuditWriter::new` to accept a `SecretsRedactor<'static>`. All four acceptance tests pass; `just check` gate passes cleanly.

### Simplify Findings
- **Reuse:** Imported `audit_prev_hash_seed` at the top of `audit_writer.rs` rather than using the full `trilithon_core::storage::helpers::audit_prev_hash_seed()` path inline. Fixed inline.
- **Quality:** `let occurred_at_ms = now_ms` and `let kind = append.event.to_string()` were no-op intermediate bindings. Inlined into struct initializer. Fixed inline.
- **Quality:** Step-narrating comments ("Step 1 — fresh row id", etc.) removed as they explained WHAT, not WHY. The ADR-0009 `prev_hash` comment was retained. Fixed inline.
- **Reuse:** `FixedClock` and `ZeroHasher` duplicated across 3 integration test binaries. Cannot be unified into a shared module due to Rust integration test binary isolation — left as-is.

### Items Fixed Inline
- Imported `audit_prev_hash_seed` at top level — `audit_writer.rs` (reuse)
- Removed no-op intermediate bindings `occurred_at_ms` and `kind` — `audit_writer.rs` (quality)
- Removed step-narrating comments — `audit_writer.rs` (quality)

### Items Left Unfixed
- `FixedClock`/`ZeroHasher` duplicated in 3 test files — Rust integration test isolation prevents a shared module (not a production concern)

## Slice 6.6
**Status:** complete
**Date:** 2026-05-09
**Summary:** Implemented `Storage::tail_audit_log` on `SqliteStorage` with static SQL using `? IS NULL OR col OP ?` double-binding, `ORDER BY id DESC` cursor pagination via `AuditRowId`, `until` exclusive bound, limit normalization (0→100, max 1000). Added `cursor_before: Option<AuditRowId>` to `AuditSelector`, fixed `until` exclusivity in `in_memory.rs`, and added 6 integration test files. Simplify pass consolidated row converters. All 11 new audit_query tests pass; `just check` gate passes.

### Simplify Findings
- **Reuse (R1):** `row_to_audit_event_row_no_prev_hash` duplicated ~32-line mapping body present in new `audit_row_from_sqlite`. Consolidated: no-prev-hash variant now delegates to `audit_row_from_sqlite` and overwrites `prev_hash` with seed. Required adding `prev_hash` to the SELECT in `record_audit_event` (safe because `canonical_json_for_audit_hash` excludes that field). Fixed inline.
- **Reuse (R2):** `open()` helper duplicated across 6 integration test files. Cannot be unified — Rust integration test binary isolation. Left unfixed.
- **Reuse (R3):** Dedicated `validate_kind()` path suggested instead of inline `AUDIT_KINDS.contains`. Rejected — full-row error context needed. Left unfixed.
- **Quality (Q4):** `"Normalise limit: 0 → default 100; cap at 1000; minimum 1."` WHAT comment removed. Fixed inline.
- **Quality (Q5/E4):** `for row in &rows { result.push(...) }` replaced with `rows.iter().map(audit_row_from_sqlite).collect()`. Fixed inline.
- **Quality (Q2):** `cursor_before` materialized as `let` before bind chain — needed for lifetime extension. Not a smell; left as-is.
- **Quality (Q3):** `DEFAULT_LIMIT`/`MAX_LIMIT` as function-local consts — acceptable; no action needed.

### Items Fixed Inline
- Consolidated `audit_row_from_sqlite` + `row_to_audit_event_row_no_prev_hash` — `sqlite_storage.rs` (reuse)
- Removed WHAT comment on limit normalization — `sqlite_storage.rs` (quality)
- Iterator collect replacing manual push loop — `sqlite_storage.rs` (quality/efficiency)

### Items Left Unfixed
- `open()` helper duplicated in 6 test files — Rust integration test binary isolation prevents shared module
- Inline `AUDIT_KINDS.contains` check — full-row error context required; dedicated helper would lose it

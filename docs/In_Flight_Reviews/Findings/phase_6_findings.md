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

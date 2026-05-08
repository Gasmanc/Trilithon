---
id: scope:area::phase-6-unfixed:legacy-uncategorized
category: scope
kind: process
location:
  area: phase-6-unfixed
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

---
id: scope:area::phase-6-unfixed:legacy-uncategorized
category: scope
kind: process
location:
  area: phase-6-unfixed
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

---
id: scope:area::phase-6-unfixed:legacy-uncategorized
category: scope
kind: process
location:
  area: phase-6-unfixed
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

none

## Slice 6.5 — FixedClock/ZeroHasher duplicated in integration tests
**Date:** 2026-05-08
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Rust integration test binaries cannot share a module via a common test-support file without a separate helper crate. The duplication is limited to test code (3 files, ~15 lines each) and has no production impact. A future test-helper crate could centralise these, but introducing a new crate is out of scope for this slice.

## Slice 6.6 — open() helper duplicated in 6 integration test files
**Date:** 2026-05-09
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Rust integration test binary isolation prevents sharing a common `open()` helper. Each test binary compiles independently and cannot import from a shared `tests/common/` module without a dedicated helper crate. The duplication is 4–5 lines per file and is test-only.

## Slice 6.6 — Inline AUDIT_KINDS.contains check in audit_row_from_sqlite
**Date:** 2026-05-09
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** A dedicated `validate_kind()` helper was considered but rejected — the error returned here needs the `kind` string for the `Integrity` detail message. Extracting it would require passing the string in and out, adding no clarity. The check is one line and self-explanatory in context.

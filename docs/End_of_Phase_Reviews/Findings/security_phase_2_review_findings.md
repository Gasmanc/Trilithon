---
id: security:area::phase-2-security-review-findings:legacy-uncategorized
category: security
kind: process
location:
  area: phase-2-security-review-findings
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

# Phase 2 — Security Review Findings

**Reviewer:** security
**Date:** 2026-05-07
**Diff range:** be773df..cfba489
**Phase:** 2

---

[WARNING] SQLITE LIKE WILDCARD INJECTION IN `tail_audit_log`
File: `core/crates/adapters/src/sqlite_storage.rs`
Lines: 691–694
Description: The `kind_glob` value is sanitised only for a trailing `*` → `%` replacement, but the caller-supplied prefix is passed directly into the LIKE parameter without escaping SQLite LIKE metacharacters `%` and `_`. Any `%` or `_` characters already present in the incoming `kind_glob` value will be treated as LIKE wildcards, broadening the filter beyond the caller's intent and potentially leaking rows from other event kinds. This is not a full SQLi (the predicate template is static and the value is bound), but it subverts the intended prefix-only matching.
Suggestion: Escape `%` and `_` in `kind_glob` before constructing the LIKE parameter. SQLite supports `ESCAPE` in LIKE: replace `%` → `\%` and `_` → `\_` in the prefix portion, then append `%`, and add `ESCAPE '\'` to the static predicate template: `kind LIKE ? ESCAPE '\'`.

[WARNING] UNPARAMETERISED DB URL CONSTRUCTION FROM PATH DISPLAY
File: `core/crates/adapters/src/sqlite_storage.rs`
Lines: 72
Description: The SQLite connection URL is constructed by embedding `data_dir.display()` directly via `format!` into the URL string: `format!("sqlite://{}/trilithon.db", data_dir.display())`. On a path containing spaces, percent-signs, or characters that are meaningful in a URL (e.g. a `#` in a directory name), `display()` does not URL-encode the path, producing a syntactically invalid sqlite URL. `SqliteConnectOptions` provides a `filename()` builder method that accepts a `Path` directly and handles encoding internally.
Suggestion: Replace `format!("sqlite://{}/trilithon.db", ...)` + `from_str` with `SqliteConnectOptions::new().filename(data_dir.join("trilithon.db"))`, which takes a `Path` and requires no string encoding.

[WARNING] INTEGRITY-CHECK ERROR DETAIL EXPOSED TO STRUCTURED LOGS
File: `core/crates/adapters/src/integrity_check.rs`
Lines: 58
Description: On an integrity failure, the raw SQLite `PRAGMA integrity_check` output is emitted verbatim as a structured `tracing::error!` field: `tracing::error!(detail = %detail, "storage.integrity_check.failed")`. PRAGMA output can include internal page numbers, row-IDs, B-tree node addresses, and partial row content. If the logging backend forwards structured events to a log aggregator accessible beyond the operator, this leaks internal storage structure and potentially partial row data.
Suggestion: Replace `%detail` with a fixed marker or a byte-length count. If the full detail is needed for forensics, log it only at `tracing::debug!` or write it to a local-only file.

[WARNING] SCHEMA MIGRATIONS TABLE VERSION OVERFLOW SILENTLY SATURATES TO 0
File: `core/crates/adapters/src/migrate.rs`
Lines: 78, 96, 115
Description: `u32::try_from(v).unwrap_or(0)` is used when converting the `i64` version from `_sqlx_migrations` to `u32`. If the stored version exceeds `u32::MAX` (due to direct database manipulation or an out-of-range sqlx version), the value silently collapses to `0`. This makes `db_version` appear to be 0, causing the downgrade guard (`db_version > embedded_max`) to always pass and `apply_migrations` to re-run all migrations on a database that actually has a far-future version.
Suggestion: Replace `unwrap_or(0)` with an explicit `Err` path: return `MigrationError::Read { source: ... }` (or a new `MigrationError::VersionOverflow`) when `try_from` fails.

[SUGGESTION] `schema_migrations` CUSTOM TABLE SHADOWS sqlx's `_sqlx_migrations` WITHOUT USE
File: `core/crates/adapters/migrations/0001_init.sql`
Lines: 4–9
Description: The migration creates a `schema_migrations` table (version, applied_at, description, checksum) but no code in the diff reads or writes it; `migrate.rs` queries only `_sqlx_migrations`. The unused table could confuse a future operator or be mistaken for the authoritative migration log, leading to a decision to bypass sqlx's own checksum-verified table.
Suggestion: Remove `schema_migrations` from `0001_init.sql` if it is not used, or add code and a trigger that keeps it in sync with `_sqlx_migrations`.

[SUGGESTION] `redaction_sites` OVERFLOW SILENTLY CLAMPS TO 0 ON READ
File: `core/crates/adapters/src/sqlite_storage.rs`
Lines: 774
Description: `u32::try_from(redaction_sites_raw).unwrap_or(0)` silently replaces a negative or out-of-range `i64` stored in the database with `0`. The `redaction_sites` field counts how many secret fields were redacted from `redacted_diff_json`. If a corrupted or adversarially written row stores a negative value, the returned `AuditEventRow` will claim zero redactions occurred for a diff that may in fact contain un-redacted secrets.
Suggestion: Return a `StorageError::Integrity` when `redaction_sites_raw < 0` rather than substituting 0, so callers cannot silently treat a tampered row as fully redacted.

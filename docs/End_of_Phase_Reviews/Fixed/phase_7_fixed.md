# Phase 7 — Fixed Findings

**Run date:** 2026-05-13T00:00:00Z  
**Total fixed:** 14

| ID | Severity | Title | File | Commit | PR | Date |
|----|----------|-------|------|--------|----|------|
| F001 | CRITICAL | CAS advance fires before Caddy load | `core/crates/adapters/src/applier_caddy.rs` | `36af1e7` | — | 2026-05-13 |
| F004 | CRITICAL | Seams not written to seams-proposed.md | `docs/architecture/seams.md` | `af38262` | — | 2026-05-13 |
| F005 | HIGH | TLS observer spawned with empty hostnames | `core/crates/adapters/src/applier_caddy.rs` | `36af1e7` | — | 2026-05-13 |
| F007 | HIGH | rollback() CAS uses snapshot.config_version as expected | `core/crates/adapters/src/applier_caddy.rs` | `36af1e7` | — | 2026-05-13 |
| F009 | HIGH | advance_config_version_if_eq missing config_version check | `core/crates/adapters/src/storage_sqlite/snapshots.rs` | `36af1e7` | — | 2026-05-13 |
| F012 | HIGH | Advisory lock Drop on panic ordering | `core/crates/adapters/src/storage_sqlite/locks.rs` | `af38262` | — | 2026-05-13 |
| F014 | WARNING | verify_equivalence maps all CaddyError to Unreachable | `core/crates/adapters/src/applier_caddy.rs` | `569b149` | — | 2026-05-13 |
| F016 | WARNING | Invalid preset JSON silently discarded during render | `core/crates/core/src/reconciler/render.rs` | `569b149` | — | 2026-05-13 |
| F017 | WARNING | Conflict audit note uses hand-rolled format! string | `core/crates/adapters/src/applier_caddy.rs` | `569b149` | — | 2026-05-13 |
| F018 | WARNING | validate() returns Ok instead of PreflightFailed per TODO spec | `core/crates/adapters/src/applier_caddy.rs` | `569b149` | — | 2026-05-13 |
| F020 | WARNING | UNIX socket path and Docker container ID unvalidated | `core/crates/core/src/reconciler/render.rs` | `569b149` | — | 2026-05-13 |
| F021 | WARNING | correlation_id silently replaced with fresh ULID on parse failure | `core/crates/adapters/src/applier_caddy.rs` | `569b149` | — | 2026-05-13 |
| F022 | WARNING | ApplyAuditNotes doc comment references wrong serialiser | `core/crates/core/src/reconciler/applier.rs` | `569b149` | — | 2026-05-13 |
| F024 | SUGGESTION | bounded_excerpt can produce output 3 bytes over maximum | `core/crates/adapters/src/applier_caddy.rs` | `569b149` | — | 2026-05-13 |
| F027 | SUGGESTION | contract-roots.toml not updated for Phase 7 public API | `docs/architecture/contract-roots.toml` | `569b149` | — | 2026-05-13 |

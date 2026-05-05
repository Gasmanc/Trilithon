# Phase 5 — Unfixed Findings

**Run date:** 2026-05-05T19:54:00Z
**Total unfixed:** 7 (0 deferred · 7 won't fix · 0 conflicts pending)

| ID | Severity | Consensus | Title | File | Status | Reason |
|----|----------|-----------|-------|------|--------|--------|
| F003 | HIGH | MAJORITY | Hardcoded `caddy_instance_id = 'local'` breaks multi-instance monotonicity | `sqlite_storage.rs:299,319` | 🚫 Won't Fix | V1 single-instance design; documented inline with ADR-0009 references. Multi-instance support is a future phase. |
| F012 | WARNING | SINGLE | Immutability triggers absent until migration 0004 runs | `migrations/0004_snapshots_immutable.sql` | 🚫 Won't Fix | `SqliteStorage::open` docstring explicitly requires callers to run migrations; `run.rs` always does. Adding a redundant assertion couples storage to migration version details. |
| F013 | WARNING | SINGLE | `sort_unstable_by` on JSON object keys — duplicate keys undefined order | `canonical_json.rs:67` | 🚫 Won't Fix | `serde_json::Map` structurally disallows duplicate keys; the scenario is impossible with `serde_json::Value`. |
| F019 | WARNING | SINGLE | `SnapshotWriter` is not a named struct — rolled into `SqliteStorage` | `sqlite_storage.rs` | 🚫 Won't Fix | Phase 5 TODO used "SnapshotWriter" descriptively; the impl as methods on `SqliteStorage` is the accepted approach. |
| F020 | WARNING | SINGLE | Monotonicity property test uses deterministic loop, not proptest | `tests/snapshot.rs:572` | 🚫 Won't Fix | Test module has comment "loop-based, no proptest dependency" — deterministic exhaustive loop accepted as substitute for V1. |
| F021 | SUGGESTION | SINGLE | Two `content_address` functions perform identical SHA-256 hashing | `canonical_json.rs` | 🚫 Won't Fix | Functions have different signatures (`&DesiredState` vs `&[u8]`); the DesiredState variant is a typed convenience wrapper, not duplication. |
| F024 | SUGGESTION | SINGLE | `actor_kind` silently discarded; hardcoded "system" on write | `sqlite_storage.rs:239,452` | 🚫 Won't Fix | Intentional V1 design with inline comment; full actor_kind plumbing is a future phase concern. |

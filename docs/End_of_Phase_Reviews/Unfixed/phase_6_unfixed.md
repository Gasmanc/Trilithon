# Phase 6 — Unfixed Findings

**Run date:** 2026-05-13
**Total unfixed:** 10 (1 deferred · 8 won't fix · 0 conflicts · 0 superseded mid-cycle)

Note: 5 findings (F002, F003, F004, F005, F009) and 1 (F008) were addressed pre-aggregate or judged invalid; recorded as `wont_fix` with rationale below rather than re-attempted.

| ID | Severity | Consensus | Title | File | Status | Reason |
|----|----------|-----------|-------|------|--------|--------|
| F002 | CRITICAL | SINGLE | Diff redaction bypasses redact_diff envelope shape | `core/crates/adapters/src/audit_writer.rs` | wont_fix | Already addressed in commit dde9dc5 — AuditWriter calls redact_diff() |
| F003 | CRITICAL | MAJORITY | Silent `"null"` fallback on redacted diff serialization | `core/crates/adapters/src/audit_writer.rs` | wont_fix | Already addressed in commit dde9dc5 — AuditWriteError::Serialization propagated |
| F004 | HIGH | PARTIAL | TLS correlation_id not restored on panic | `core/crates/adapters/src/tracing_correlation.rs` | wont_fix | Already addressed in commit dde9dc5 — CorrelationGuard RAII drop-guard |
| F005 | HIGH | PARTIAL | Bypass guard does not cover cli/core crates | `core/crates/adapters/tests/audit_writer_no_bypass.rs` | wont_fix | Already addressed in commit dde9dc5 — cli/src/ in scan path; core/src is the trait-impl home for InMemoryStorage and would generate false positives |
| F006 | HIGH | MAJORITY | Slice 6.2 types not wired into adapter path | `core/crates/core/src/audit/row.rs`, `core/crates/core/src/storage/types.rs` | deferred | Large multi-file type-system refactor touching `Storage` trait signatures. Phase 7+ is already merged using the existing `storage::types` versions; rewriting now would conflict with downstream code and require coordinated migration. Track as architecture cleanup in a dedicated slice. |
| F008 | HIGH | SINGLE | RFC 6901 JSON Pointer decode order incorrect | `core/crates/core/src/schema/mod.rs` | wont_fix | Reviewer was incorrect: existing `~1` then `~0` order is RFC 6901-compliant (verified: `~01` correctly decodes to `~1`). Confirmed at the time of commit dde9dc5 (FIX 4 — no-op). |
| F009 | HIGH | SINGLE | No production CiphertextHasher implementation | `core/crates/adapters/src/sha256_hasher.rs` | wont_fix | Already addressed in commit dde9dc5 — `Sha256AuditHasher` lives in adapters and is re-exported. F013 follow-up adds the low-entropy security note on the trait. |
| F020 | WARNING | SINGLE | AuditEvent enum has variants beyond Tier 1 spec | `core/crates/core/src/audit/event.rs` | wont_fix | Phase 7+ code already merged on `main` emits these additional variants (drift, mutation rebase, policy presets, etc.). Reverting them would break downstream slices that depend on the vocabulary. The expansion is now load-bearing infrastructure. |
| F021 | WARNING | SINGLE | Background-task correlation_layer not registered in CLI | `core/crates/cli/src/run.rs` | deferred | Per-iteration `with_correlation_span` wrapping belongs inside each background loop (`integrity_check::run_integrity_loop`, `reconnect::reconnect_loop`, drift detector `run`) — not at the spawn site, where one wrapper would tag every iteration with the same id. Documented at the spawn site; per-loop wrapping deferred to the slice that next touches each loop. |
| F031 | SUGGESTION | SINGLE | Unused http/tower deps in adapters | `core/crates/adapters/Cargo.toml` | wont_fix | Accepted as Phase 9 scaffolding debt per the aggregate's recommendation. Adding a feature flag here adds complexity for a deps-set that's about to be used in the next phase. |

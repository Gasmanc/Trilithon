# Round 10 — Phase 8 Adversarial Review

**Design summary:** Phase 8 adds a drift-detection loop that periodically fetches the running Caddy config, diffs it against the latest desired state, deduplicates on `running_state_hash`, and offers three resolution paths (adopt, reapply, defer). The detector shares an `apply_mutex` with Phase 7's applier to skip ticks during in-flight applies.

**Prior rounds:** 87 findings across rounds 1–9. 11 accepted as known risk. Key constraints adopted include: `/storage/.*` narrowed to `/storage/acme/` and `/storage/ocsp/`, three new `Mutation` variants defined in Slice 8.4, `Arc<Self>` ownership model for `DriftDetector`, and `DriftDeferred` as a no-op at the apply path.

---

## Findings

### [HIGH] F088: Constraint rollback — `/storage/.*` broad pattern still in the design document
**Category:** Assumption (constraint adopted in review but not propagated to design)
**Trigger:** Line 173 of the TODO still contains `"^/storage/.*"` — the broad pattern that prior rounds narrowed to `^/storage/acme/` and `^/storage/ocsp/`.
**Consequence:** All `/storage/` paths silently discarded as Caddy-managed, including user-owned storage keys. Permanently silent drift.
**Suggested mitigation:** Replace with two patterns: `"^/storage/acme(/.*)?$"` and `"^/storage/ocsp(/.*)?$"`. Add test `does_not_match_storage_root`.

### [HIGH] F089: New Mutation variants from Slice 8.4 have no applier match-arm coverage
**Category:** Composition failure (Phase 8 → Phase 7 boundary)
**Trigger:** Slice 8.4 introduces `ReplaceDesiredState`, `ReapplySnapshot`, `DriftDeferred` — Phase 7 applier's match arms not updated.
**Consequence:** If wildcard arm exists, adopt/reapply mutations silently discarded. System appears to resolve drift but Caddy remains diverged.
**Suggested mitigation:** Add applier match arms to Slice 8.4's files list. Add cross-phase integration test.

### [MEDIUM] F090: `Arc::unwrap_or_clone(detector)` wrong idiom for shared ownership
**Category:** Logic flaw (assumption)
**Trigger:** CLI wiring spec uses `Arc::unwrap_or_clone` which has consuming semantics on single-owner path.
**Suggested mitigation:** Use explicit `Arc::clone` into task closure.

### [MEDIUM] F091: `record()` writes audit + drift-event non-atomically despite atomicity constraint
**Category:** Composition failure (Phase 6 → Phase 8 boundary)
**Trigger:** Steps 3 and 4 in Slice 8.6 are sequential awaits with no transaction boundary.
**Suggested mitigation:** Wrap in single SQLite transaction.

### [LOW] F092: `DriftDetectorConfig` interval validation not specified at construction site
**Category:** Logic flaw
**Trigger:** `from_settings` doesn't specify validation or error handling for out-of-range intervals.
**Suggested mitigation:** `from_settings` returns `Result` and rejects intervals outside [10, 3600].

## Summary

| Critical | High | Medium | Low |
|----------|------|--------|-----|
| 0 | 2 | 2 | 1 |

**Top concern:** F088 — `/storage/.*` broad pattern remains in authoritative design despite constraint adoption.

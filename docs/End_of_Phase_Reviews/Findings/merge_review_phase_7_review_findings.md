---
phase_id: 7
review_kind: merge-review
mode: catch-up
diff_base: ddda14690f401c112c8c6716f4d71601bdb33d0d
diff_head: 838828d71bbde3d0a972e1d42126e7ac3fca5456
phase_tag: cross-cutting
deterministic_findings: 3
llm_findings: 2
total_findings: 5
blocking: false
invoked_by: manual
invoked_at: 2026-05-10T00:00:00Z
note: >
  Catch-up mode — Phase 7 was already merged to main before this review ran.
  Verdict cannot block. Any finding that would have been BLOCKING is elevated
  to a critical-severity super-finding per the catch-up protocol.
---

# Phase 7 Merge Review

**Mode:** catch-up (phase already merged to main before review)
**Diff:** `ddda146..HEAD` — 52 files, ~7 945 lines
**Phase tag:** `[cross-cutting]` (4 standard slices, 4 cross-cutting slices)

## Summary

```
Phase Merge Review — phase-7
══════════════════════════════════════════════════════
Mode:               catch-up
Diff:               ddda146..838828d  (52 files, 7 945 lines)
Phase tag:          [cross-cutting]

Deterministic findings:  3
LLM findings:            2
                         ─
Total new findings:      5
Observed again:          0
Filtered (accepted):     0

Severity:
  critical:  1    ← would have BLOCKED MERGE
  high:      2
  medium:    1
  low:       1

Verdict: BLOCKING (catch-up: cannot block — findings filed for tracking)
```

---

## Findings

### F-PMR7-001 — Proposed seams not written to `seams-proposed.md` (critical)

```yaml
id: seam-coverage:area::phase-7-cross-cutting-seams:proposed-seam-not-ratified
severity: critical
category: seam-coverage
location: docs/architecture/seams-proposed.md
finding_kind: proposed-seam-not-ratified
do_not_autofix: false
phase_introduced: 7
```

**Description:**
The Phase 7 tagging audit (accepted 2026-05-09) identified five architectural seams
introduced by the cross-cutting slices 7.4–7.7 and recorded them as PROPOSED additions.
Per the Foundation 2 rules, proposed seams must be written to `seams-proposed.md` before
merge, and ratified into `seams.md` by `/phase-merge-review`. Neither step was performed:

- `seams-proposed.md` — still empty (`proposed_seams: []`)
- `seams.md` — still empty (`seams: []`)
- `tests/cross_phase/` — directory exists but contains no test files

The five unratified seams are:

| Seam ID | Proposed by | Description |
|---|---|---|
| `applier-caddy-admin` | slice 7.4 | Applier ↔ CaddyClient boundary |
| `applier-audit-writer` | slice 7.4 | Applier ↔ AuditWriter terminal-row contract |
| `snapshots-config-version-cas` | slice 7.5 | Storage CAS ↔ applier config_version pointer-advance |
| `apply-lock-coordination` | slice 7.6 | In-process mutex + SQLite advisory lock per caddy_instance_id |
| `apply-audit-notes-format` | slice 7.7 | ApplyAuditNotes serde wire format ↔ audit-log query consumers |

**Why critical:** The skip-merge-review rule (`seam-coverage:*:proposed-seam-not-ratified`)
is BLOCKING per the skill specification. In catch-up mode the merge has already landed, so
this is elevated to a critical super-finding.

**Fix:** In the next phase that touches the apply path (or a dedicated seam-ratification
micro-phase), populate `seams-proposed.md` with the five entries above, ratify them into
`seams.md`, and add stub test files to `tests/cross_phase/` for each seam.

---

### F-PMR7-002 — Duplicate canonical-JSON sort logic: `applier_caddy.rs` (high)

```yaml
id: reuse-miss:core/crates/adapters/src/applier_caddy.rs::sort_keys+notes_to_string:canonical-value-sort-duplicated
severity: high
category: reuse-miss
location: core/crates/adapters/src/applier_caddy.rs:93-112
finding_kind: reuse-miss
do_not_autofix: true
phase_introduced: 7
```

**Description:**
`applier_caddy.rs` defines a free-standing `sort_keys(v: serde_json::Value) -> Value`
function (lines 102–113) that recursively sorts JSON object keys, plus a `notes_to_string`
wrapper (lines 93–99). This duplicates the `canonicalise_value` logic already present in
`core::canonical_json::canonicalise_value` (which is `pub(crate)` in core).

Crucially, `core::reconciler::render::canonical_json_bytes(value: &Value) -> Vec<u8>`
is already `pub` and takes a `&serde_json::Value` — it could replace `sort_keys` +
`notes_to_string` without any visibility change. The `applier_caddy.rs` comment at line
90–92 acknowledges the duplication ("mirrors the canonical-JSON intent without requiring
the core `canonicalise_value` helper to be public") but misses the already-public
`render::canonical_json_bytes`.

**Consequence:** Two separate implementations of the same sort guarantee. If the canonical
format ever changes, `applier_caddy.rs` and `tls_observer.rs` (see F-PMR7-003) must be
updated in parallel with `core::canonical_json`.

**Fix:** Replace `notes_to_string` + `sort_keys` in `applier_caddy.rs` with a call to
`trilithon_core::reconciler::render::canonical_json_bytes`. Return type changes from
`String` to `Vec<u8>` (then `String::from_utf8_lossy` or `from_utf8` for storage).
Alternatively, expose a `canonical_value_to_string` helper from `core::canonical_json`
and use it in both adapter files.

---

### F-PMR7-003 — Duplicate canonical-JSON sort logic: `tls_observer.rs` (high)

```yaml
id: reuse-miss:core/crates/adapters/src/tls_observer.rs::TlsIssuanceObserver::sort_keys+notes_to_string:canonical-value-sort-duplicated
severity: high
category: reuse-miss
location: core/crates/adapters/src/tls_observer.rs:61-83
finding_kind: reuse-miss
do_not_autofix: true
phase_introduced: 7
```

**Description:**
`TlsIssuanceObserver` contains `notes_to_string` and `sort_keys` as instance methods
(lines 61–83 of `tls_observer.rs`) that are structurally identical to the same pair of
functions in `applier_caddy.rs`. This is the second copy of the same pattern, crossing
the two-use threshold.

The fix is the same as F-PMR7-002: consolidate both callers onto
`render::canonical_json_bytes` or a newly-exposed `canonical_value_to_string` helper,
then delete all local copies.

---

### F-PMR7-004 — Doc comment on `ApplyAuditNotes` references non-applicable API (medium)

```yaml
id: contract-drift:core/crates/core/src/reconciler/applier.rs::ApplyAuditNotes:doc-references-wrong-serialiser
severity: medium
category: contract-drift
location: core/crates/core/src/reconciler/applier.rs:251
finding_kind: contract-marker-drift
do_not_autofix: false
phase_introduced: 7
```

**Description:**
The doc comment on `ApplyAuditNotes` (line 251, `applier.rs`) states:

> "Serialised via `trilithon_core::canonical_json::to_canonical_bytes` before storage
> so that the JSON is deterministic and content-addressable."

However, `to_canonical_bytes` only accepts `&DesiredState`, not `&ApplyAuditNotes`.
The actual serialisation happens via the local `notes_to_string` helpers in `applier_caddy.rs`
and `tls_observer.rs`. The doc comment is contractually misleading: Phase 9/13 authors
reading this will expect `ApplyAuditNotes` bytes to be canonicalised to the same spec as
`DesiredState`, but the actual implementation bypasses that path entirely.

**Fix:** Correct the doc comment to describe the actual serialisation path (e.g., "sorted
keys via `render::canonical_json_bytes`" once F-PMR7-002/003 are applied, or "sorted keys
via the internal `notes_to_string` helper" until then).

---

### F-PMR7-005 — `contract-roots.toml` not updated for Phase 7 public surface (low)

```yaml
id: contract-drift:docs/architecture/contract-roots.toml::phase-7-symbols:contract-roots-not-updated
severity: low
category: contract-drift
location: docs/architecture/contract-roots.toml
finding_kind: contract-drift
do_not_autofix: false
phase_introduced: 7
```

**Description:**
Phase 7 introduced a substantial new public API surface across `core` and `adapters`:

**core:**
- `trilithon_core::reconciler::render::CaddyJsonRenderer` (trait)
- `trilithon_core::reconciler::render::DefaultCaddyJsonRenderer`
- `trilithon_core::reconciler::render::RenderError`
- `trilithon_core::reconciler::render::canonical_json_bytes`
- `trilithon_core::reconciler::applier::Applier` (trait)
- `trilithon_core::reconciler::applier::ApplyOutcome`
- `trilithon_core::reconciler::applier::AppliedState`
- `trilithon_core::reconciler::applier::ReloadKind`
- `trilithon_core::reconciler::applier::ApplyFailureKind`
- `trilithon_core::reconciler::applier::ApplyError`
- `trilithon_core::reconciler::applier::ApplyAuditNotes`
- `trilithon_core::reconciler::applier::AppliedStateTag`
- `trilithon_core::reconciler::capability_check::CapabilityCheckError`
- `trilithon_core::reconciler::capability_check::check_against_capability_set`

**adapters:**
- `trilithon_adapters::applier_caddy::CaddyApplier`
- `trilithon_adapters::storage_sqlite::snapshots::current_config_version`
- `trilithon_adapters::storage_sqlite::snapshots::advance_config_version_if_eq`
- `trilithon_adapters::storage_sqlite::locks::LockError`
- `trilithon_adapters::storage_sqlite::locks::AcquiredLock`
- `trilithon_adapters::storage_sqlite::locks::acquire_apply_lock`
- `trilithon_adapters::tls_observer::TlsIssuanceObserver`

None of these appear in `contract-roots.toml` (which still has only the example
placeholder). The tagging audit explicitly called out these as "planned contract additions"
for each slice. Without registry entries, `/coherence-audit` cannot track drift for any of
these symbols in future phases.

**Severity:** low because the contract registry is uniformly empty across all phases (this
is a pre-existing gap, not a Phase 7-specific regression). However Phase 7 is the first
phase that introduces cross-cutting contracts that future phases (Phase 9 HTTP layer,
Phase 13 query path) will structurally depend on — leaving them unregistered increases
the risk of silent drift.

**Fix:** Populate `contract-roots.toml` with at minimum the `core` symbols listed above
(Applier trait, ApplyOutcome, ApplyAuditNotes, AppliedState, ReloadKind, ApplyError).
Run `cargo xtask registry-extract` (when xtask is available) to regenerate `contracts.md`.

---

## Deterministic Check Results

### 4a. Contract drift detection — SKIP
`xtask registry-extract` not yet implemented in this project. Contract registry is
uniformly empty. Finding F-PMR7-005 captures the gap.

### 4b. Contract marker churn
No `// contract:` markers were added or removed in the Phase 7 diff. PASS.

### 4c. Contract-roots changes
`docs/architecture/contract-roots.toml` was NOT modified in the Phase 7 diff.
Given the volume of new public API surface (finding F-PMR7-005), this is a gap.

### 4d. Seam-test compliance
`seams.md` has no entries. No seam tests to verify. However, four cross-cutting slices
(7.4–7.7) introduced seams that were acknowledged in the tagging audit but never written
to `seams-proposed.md`. This is captured by F-PMR7-001.

### 4e. Proposed-seam ratification
`seams-proposed.md` is empty. Five seams were proposed in the tagging audit but never
written here. Per the protocol, `[cross-cutting]` seams MUST be ratified at merge.
Captured by F-PMR7-001 (critical — would have blocked merge).

### 4f. Cross-cutting compliance (`[cross-cutting]` phase)
`xtask cross-cutting-matrix` not implemented. Manual review:
- **Audit-log emissions:** all four cross-cutting slices emit audit rows consistently
  using the established `config.applied`, `config.apply-failed`, `mutation.conflicted`
  kind strings. PASS.
- **Tracing events:** `apply.started`, `apply.succeeded`, `apply.failed` tracing spans
  present. PASS.
- **Error propagation pattern:** `?` used throughout; errors surfaced via `ApplyError`
  enum. PASS.

### 4g. Orphaned-caller scan
`xtask orphan-scan` not implemented. `cargo check` exited 0 — no orphaned callers
in-workspace. PASS.

### 4h. Invariant cross-check
`xtask invariant-check` not implemented. `contracts-invariants.md` is empty (no symbols
documented). No orphaned invariants to check. PASS (vacuously).

## Gate

```
cargo check --workspace --all-targets: EXIT 0
```

The workspace compiles without errors. All deterministic checks that could be run passed.

---

## Disposition of Findings

| ID | Severity | Title | Action |
|---|---|---|---|
| F-PMR7-001 | critical | Proposed seams not written or ratified | File to Unfixed — fix in next apply-path phase |
| F-PMR7-002 | high | Duplicate sort_keys in applier_caddy.rs | File to Unfixed — fix when touching that file |
| F-PMR7-003 | high | Duplicate sort_keys in tls_observer.rs | File to Unfixed — fix alongside F-PMR7-002 |
| F-PMR7-004 | medium | ApplyAuditNotes doc references wrong serialiser | File to Unfixed — fix when applying F-PMR7-002 |
| F-PMR7-005 | low | contract-roots.toml not updated | File to Unfixed — update in a seam-ratification pass |

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| F-PMR7-001 | Seams not written to seams-proposed.md | ✅ Fixed | `af38262` | — | 2026-05-13 | 5 seams ratified into seams.md; 5 stub tests added |
| F-PMR7-002 | Duplicate notes_to_string (applier_caddy) | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation (shared audit_notes module) |
| F-PMR7-003 | Duplicate notes_to_string (tls_observer) | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation (shared audit_notes module) |
| F-PMR7-004 | ApplyAuditNotes doc comment wrong serialiser | ✅ Fixed | `569b149` | — | 2026-05-13 | |
| F-PMR7-005 | contract-roots.toml not updated | ✅ Fixed | `569b149` | — | 2026-05-13 | Phase 7 core contracts added |
| — | Migration filename mismatch | 🔕 Superseded | — | — | — | Doc-only, out of aggregate scope |

# Phase 7 Merge Review — Unfixed Findings

Source: `docs/End_of_Phase_Reviews/Findings/merge_review_phase_7_review_findings.md`
Review date: 2026-05-10
Mode: catch-up (phase already merged)

---

## F-PMR7-001 — Proposed seams not written or ratified (critical)

```yaml
id: seam-coverage:area::phase-7-cross-cutting-seams:proposed-seam-not-ratified
severity: critical
category: seam-coverage
location: docs/architecture/seams-proposed.md
finding_kind: proposed-seam-not-ratified
do_not_autofix: false
phase_introduced: 7
```

The tagging audit (accepted 2026-05-09) identified five seams for Phase 7 cross-cutting
slices. None were written to `seams-proposed.md` before merge, and none were ratified into
`seams.md`. `tests/cross_phase/` is empty.

Unratified seams: `applier-caddy-admin`, `applier-audit-writer`,
`snapshots-config-version-cas`, `apply-lock-coordination`, `apply-audit-notes-format`.

**Fix:** Write all five to `seams-proposed.md`, ratify into `seams.md`, add stub test
files to `tests/cross_phase/` with at least one `assert` per seam. Do this in the next
phase that touches the apply path.

---

## F-PMR7-002 — Duplicate canonical-JSON sort logic: `applier_caddy.rs` (high)

```yaml
id: reuse-miss:core/crates/adapters/src/applier_caddy.rs::sort_keys+notes_to_string:canonical-value-sort-duplicated
severity: high
category: reuse-miss
location: core/crates/adapters/src/applier_caddy.rs:93-112
finding_kind: reuse-miss
do_not_autofix: true
phase_introduced: 7
```

`sort_keys` + `notes_to_string` in `applier_caddy.rs` duplicate `canonicalise_value` from
`core::canonical_json`. Replace with `render::canonical_json_bytes(&Value)` which is
already `pub`.

---

## F-PMR7-003 — Duplicate canonical-JSON sort logic: `tls_observer.rs` (high)

```yaml
id: reuse-miss:core/crates/adapters/src/tls_observer.rs::TlsIssuanceObserver::sort_keys+notes_to_string:canonical-value-sort-duplicated
severity: high
category: reuse-miss
location: core/crates/adapters/src/tls_observer.rs:61-83
finding_kind: reuse-miss
do_not_autofix: true
phase_introduced: 7
```

Second copy of the same `sort_keys` + `notes_to_string` pattern as F-PMR7-002.
Fix alongside F-PMR7-002 by consolidating onto `render::canonical_json_bytes`.

---

## F-PMR7-004 — `ApplyAuditNotes` doc references wrong serialiser (medium)

```yaml
id: contract-drift:core/crates/core/src/reconciler/applier.rs::ApplyAuditNotes:doc-references-wrong-serialiser
severity: medium
category: contract-drift
location: core/crates/core/src/reconciler/applier.rs:251
finding_kind: contract-marker-drift
do_not_autofix: false
phase_introduced: 7
```

Doc comment claims `ApplyAuditNotes` is "serialised via
`trilithon_core::canonical_json::to_canonical_bytes`" but that function only accepts
`&DesiredState`. Actual serialisation uses local `notes_to_string`. Update doc comment
when F-PMR7-002/003 are resolved.

---

## F-PMR7-005 — `contract-roots.toml` not updated for Phase 7 public surface (low)

```yaml
id: contract-drift:docs/architecture/contract-roots.toml::phase-7-symbols:contract-roots-not-updated
severity: low
category: contract-drift
location: docs/architecture/contract-roots.toml
finding_kind: contract-drift
do_not_autofix: false
phase_introduced: 7
```

14 core symbols and 7 adapter symbols introduced in Phase 7 are not registered in
`contract-roots.toml`. Key symbols: `Applier` trait, `ApplyOutcome`, `ApplyAuditNotes`,
`AppliedState`, `ReloadKind`, `ApplyError`, `CaddyApplier`, `TlsIssuanceObserver`.

**Fix:** Add these to `contract-roots.toml` and regenerate `contracts.md` via
`cargo xtask registry-extract` (when available). Priority: populate `core` symbols before
Phase 9 ships (Phase 9 HTTP layer structurally depends on `Applier` + `ApplyOutcome`).

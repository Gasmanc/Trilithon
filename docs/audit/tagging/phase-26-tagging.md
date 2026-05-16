# Phase 26 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, architecture.md, trait-signatures.md (referenced for `core::storage::Storage` / `core::secrets::SecretsVault`), PRD.md (§6.12 T2.12), ADR-0009, ADR-0014 (and ADR index), docs/todo/phase-26-backup-and-restore.md, seams.md, seams-proposed.md, contract-roots.toml, contracts.md, bundle-format-v1.md
**Slices analysed:** 8

## Proposed Tags

### 26.1: `POST /api/v1/backup` handler with optional access-log inclusion
**Proposed tag:** [standard]
**Reasoning:** Touches two tightly related modules — a `cli` HTTP handler and an `adapters` packager — but both exist purely to wrap the already-built Phase 25 `bundle_packager` with one optional tar member. It adds I/O confined to the export adapter, introduces no new trait, and crosses no layer boundary not already crossed by every HTTP handler in `cli`. It emits only the pre-existing `export.bundle` audit kind and standard `http.request.*` tracing events, so it sets no new convention others must follow.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 26.2: Restore pipeline scaffolding and seven-step error type
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice spans `core` (the `RestoreError`/`RestoreOutcome` shapes and `core::restore` module) and `adapters` (the `restore::pipeline` orchestrator), and the orchestrator consumes the shared `core::storage::Storage` and `core::secrets::SecretsVault` traits across the layer boundary. It establishes the seven-step `RestoreError` taxonomy and the `restore_bundle` entry point that all of slices 26.3–26.7 depend on — a foundational migration-style contract for the rest of the phase. It references ADR-0009, ADR-0014, the bundle format spec, and PRD T2.12.
**Affected seams:** PROPOSED: restore-pipeline-storage-vault — "Restore pipeline ↔ Storage + SecretsVault" — the seven-step `restore_bundle` boundary where bundle bytes become an atomic data-directory swap, exercising `core::storage::Storage` and `core::secrets::SecretsVault`.
**Planned contract additions:** `trilithon_core::restore::RestoreError`, `trilithon_core::restore::RestoreOutcome`, `trilithon_adapters::restore::pipeline::restore_bundle`, `trilithon_adapters::restore::pipeline::RestoreRequest`, `trilithon_adapters::restore::pipeline::RestoreOptions`
**Confidence:** high
**If low confidence, why:** n/a

### 26.3: Restore steps 1–2 — manifest compatibility check + master-key unwrap
**Proposed tag:** [standard]
**Reasoning:** Both step modules live in a single crate (`adapters::restore::steps`), implement no trait, and perform no I/O — `verify` is a pure match on `schema_version` and `unwrap` delegates to the Phase 25 `unwrap_master_key` already built. It extends the `RestoreError` taxonomy established by 26.2 rather than defining new shared conventions, and emits no audit or tracing events. The cross-references are only ADR-0014 and architecture §14.
**Affected seams:** none (operates inside the restore-pipeline-storage-vault seam proposed for 26.2)
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 26.4: Restore steps 3–4 — audit-log content-address + snapshot tree validation
**Proposed tag:** [standard]
**Reasoning:** Both validators are pure functions in a single crate (`adapters::restore::steps`), with no trait, no I/O, and no audit/tracing emission. They re-use `core::export::deterministic::write_canonical_compact` and an SHA-256 helper rather than introducing new abstractions. The content-addressing rule they enforce is owned by ADR-0009 and the bundle format spec — this slice consumes that contract, it does not define one others follow.
**Affected seams:** none (operates inside the proposed restore-pipeline seam)
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 26.5: Restore steps 5–7 — preflight, atomic swap, failure leaves state untouched
**Proposed tag:** [cross-cutting]
**Reasoning:** Although the new files live in `adapters`, the preflight step calls into the `core::preflight` engine and the swap step performs the load-bearing atomic data-directory replacement that the whole restore correctness story rests on. The slice's own documentation flags that it triggers a pending architecture §6.6 audit-vocabulary addition (`system.restore-applied`), an unresolved open question that affects shared convention. It references ADR-0009, architecture §14, and hazard H9, and the preflight cross-call effectively crosses into the Phase 12 preflight contract.
**Affected seams:** none ratified; operates inside the proposed restore-pipeline-storage-vault seam. The `core::preflight::run_against` cross-call is an existing-engine reuse, not a new seam.
**Planned contract additions:** none (the `restore_bundle` entry point is the contract; step modules are internal)
**Confidence:** medium
**If low confidence, why:** Whether the preflight reuse is heavyweight enough alone to be cross-cutting is debatable, but the §6.6 vocabulary touch and the irreversible-swap risk surface push it over the line.

### 26.6: Cross-machine handoff — `installation_id` lifecycle and audit rows
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice spans `core` (new `installation_id.rs` type plus new `AuditEvent::RestoreApplied`/`RestoreCrossMachine` variants in `core::audit`) and `adapters` (the handoff logic). Crucially it modifies the shared `core::audit::AuditEvent` enum and MUST add `system.restore-applied` and `system.restore-cross-machine` to the authoritative architecture §6.6 audit vocabulary in the same commit — a tracing/audit convention every audit consumer follows. It introduces a new persisted identity type and references ADR-0009, architecture §6.6, and PRD T2.12.
**Affected seams:** PROPOSED: restore-installation-id-audit — "Restore cross-machine handoff ↔ AuditEvent vocabulary" — exercising `core::audit::AuditEvent::RestoreApplied`, `core::audit::AuditEvent::RestoreCrossMachine`, and `core::installation_id::InstallationId`. (Alternatively folded into the restore-pipeline seam; recorded separately because the `AuditEvent` enum change is a distinct shared-contract surface.)
**Planned contract additions:** `trilithon_core::installation_id::InstallationId`, `trilithon_core::audit::AuditEvent::RestoreApplied`, `trilithon_core::audit::AuditEvent::RestoreCrossMachine`
**Confidence:** high
**If low confidence, why:** n/a

### 26.7: `POST /api/v1/restore` handler and CLI subcommand
**Proposed tag:** [standard]
**Reasoning:** Both surfaces — the Axum multipart handler and the `clap` subcommand — live in the single `cli` crate and are thin drivers over the already-built `restore_bundle` pipeline; the heavy lifting and all layer-crossing already happened in 26.2–26.6. It maps the existing `RestoreError` variants to HTTP statuses and exit codes without defining new shared conventions, and emits only audit kinds and tracing events introduced by earlier slices. This is the canonical shape of a one-crate handler-plus-CLI slice.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 26.8: Web UI Backup-and-restore page with confirmation gate
**Proposed tag:** [trivial]
**Reasoning:** A single new React component plus its Vitest file under `web/src/features/backup/`, in one module, with no new trait, no cross-layer dependency, no I/O (the page only invokes injected `onCreateBackup`/`onRestore` props), and no audit or tracing events. It is a self-contained presentational component with a confirmation gate; nothing else in the codebase depends on its internals.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

## Summary
- 1 trivial / 4 standard / 3 cross-cutting / 0 low-confidence

## Notes

- The seam registry (`seams.md`) currently holds only Phase 7 apply-path seams; Phase 26 introduces the first restore-related boundaries. Two proposed seams are noted (restore-pipeline-storage-vault for 26.2, restore-installation-id-audit for 26.6). `/tag-phase` only stages proposed seams in `seams-proposed.md`; `/phase-merge-review` ratifies. The implementer may reasonably consolidate both into one `restore-pipeline` seam — they are recorded separately because the `core::audit::AuditEvent` enum is a distinct shared-contract surface from the `restore_bundle` orchestrator.
- `contracts.md` is currently empty and `contract-roots.toml` lists only Phase 7 reconciler symbols. The "Planned contract additions" above are candidates for `contract-roots.toml`; adding them is itself a contract change that `/phase-merge-review` must review. The strongest candidates are the public restore entry point (`restore_bundle`, `RestoreError`, `RestoreOutcome`) and the new `AuditEvent` variants, since the audit-kind vocabulary is already a cross-phase authority surface.
- Slice 26.6 carries a hard same-commit obligation: architecture §6.6 must gain `system.restore-applied` and `system.restore-cross-machine`. The open question in the TODO file (whether `RestoreApplied` maps to a new kind or an existing one) must be resolved by the §6.6 vocabulary authority before that commit lands — this is the main reason 26.5 and 26.6 are cross-cutting.
- Effort estimates in the slice plan (4–7 ideal-eng-hours) correlate loosely but not perfectly with the tags: 26.5 (7h) and 26.2 (5h) are cross-cutting as expected, but 26.6 (4h) is cross-cutting on convention-impact grounds despite low effort, and 26.4 (6h) is standard despite higher effort because it is pure single-crate validation logic.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.

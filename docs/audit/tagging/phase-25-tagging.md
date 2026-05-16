# Phase 25 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, docs/architecture/architecture.md, docs/architecture/trait-signatures.md, docs/planning/PRD.md, docs/adr/ (0009, 0014 and the full set), docs/todo/phase-25-config-export.md, docs/architecture/seams.md, docs/architecture/contract-roots.toml, docs/architecture/contracts.md, docs/architecture/bundle-format-v1.md
**Slices analysed:** 13

## Proposed Tags

### 25.1: Deterministic JSON-ordering helper
**Proposed tag:** [standard]
**Reasoning:** Adds one new module (`core::export`) in a single crate (`core`), with a pure canonical-JSON writer and a new error type — no traits, no I/O, no cross-layer dependency. It is more than [trivial] because it creates a new module root that two downstream slices (25.2, 25.7) consume and depend on byte-for-byte; the canonical-JSON ordering rule is a shared determinism convention enforced at this single boundary. It does not span crates or introduce tracing/audit conventions, so it is not [cross-cutting].
**Affected seams:** none
**Planned contract additions:** `trilithon_core::export::deterministic::write_canonical_pretty`, `write_canonical_compact`, `CanonicalJsonError`
**Confidence:** high
**If low confidence, why:** n/a

### 25.2: Caddy JSON serialiser and integration test
**Proposed tag:** [standard]
**Reasoning:** Adds one exporter module in `core` plus an integration test in `adapters/tests/`; the test directory is test-only and does not move production code across the layer boundary. It consumes the 25.1 helper and the existing Phase 4 `DesiredState::render_caddy_json` rather than modifying a shared trait. The work is self-contained to the export feature within `core` (production code) with a single adapter-side test harness, matching [standard].
**Affected seams:** none
**Planned contract additions:** `trilithon_core::export::caddy_json::export_caddy_json`, `ExportError`
**Confidence:** high
**If low confidence, why:** n/a

### 25.3: Caddyfile printer with snippet deduplication and translation reference
**Proposed tag:** [standard]
**Reasoning:** Adds a printer and snippet-dedup helper in a single crate (`core`), plus a translation-reference doc and a doc lint — all within one layer with no I/O and no trait modification. The `LossyWarning` enum is new public surface but is consumed only by export handlers in 25.9, not a shared cross-phase trait. References PRD T2.9 and hazard H7 but only one ADR (0009), so it does not reach the 3+ ADR/PRD threshold for [cross-cutting].
**Affected seams:** none
**Planned contract additions:** `trilithon_core::caddyfile::printer::print`, `LossyWarning`, `PrintResult`, `caddyfile::printer::snippets::{Snippet, SnippetSet, extract_snippets}`
**Confidence:** medium
**If low confidence, why:** `LossyWarning`'s stable-id contract is consumed across the HTTP layer (25.9) and a round-trip test (25.12), which gives it mild cross-slice contract weight, but it stays inside Phase 25.

### 25.4: Bundle manifest schema (Rust type and JSON Schema)
**Proposed tag:** [standard]
**Reasoning:** Adds `BundleManifest` and supporting types in one crate (`core`) plus a published JSON Schema document — pure data types, no traits, no I/O. It is not [trivial] because the types are an authoritative on-disk format tracked against `bundle-format-v1.md` and consumed by Phase 26's restore path, making them durable public surface. It touches only one layer and one crate, so it is not [cross-cutting].
**Affected seams:** none
**Planned contract additions:** `trilithon_core::export::manifest::{BundleManifest, RedactionPosture, SecretsEncryption, KdfParams}`
**Confidence:** high
**If low confidence, why:** n/a

### 25.5: Deterministic tar packer
**Proposed tag:** [standard]
**Reasoning:** Adds one packer module in a single crate (`adapters`), introduces I/O-adjacent crates (`tar`, `flate2`) in that one adapter, and exposes a typed `PackError`. No traits are implemented or modified, and the code stays inside the `adapters` layer. This matches [standard]: one crate, may add I/O in a single adapter, one layer.
**Affected seams:** none
**Planned contract additions:** `trilithon_adapters::export::tar_packer::{pack, TarMember, PackError}`
**Confidence:** high
**If low confidence, why:** n/a

### 25.6: Master-key wrap (Argon2id + XChaCha20-Poly1305)
**Proposed tag:** [standard]
**Reasoning:** Adds wrap/unwrap functions in a single `adapters` module using `argon2`, `chacha20poly1305`, `rand` — cryptographic logic confined to one crate and one layer, no traits, no shared convention. References ADR-0014 and `bundle-format-v1.md` §8 but not 3+ ADRs/PRD IDs. Self-contained to the export feature inside `adapters`, matching [standard].
**Affected seams:** none
**Planned contract additions:** `trilithon_adapters::export::master_key_wrap::{wrap_master_key, unwrap_master_key, WrapError}`
**Confidence:** high
**If low confidence, why:** n/a

### 25.7: Bundle exporter and named determinism test
**Proposed tag:** [cross-cutting]
**Reasoning:** The bundle pipeline spans two crates and crosses the core↔adapters boundary: `core::export::bundle` does pure assembly while `adapters::export::bundle_packager` orchestrates I/O against the `Storage` trait, the master-key wrap, and the tar packer. It composes the outputs of four prior slices (25.1, 25.4, 25.5, 25.6), depends on the `core::storage::Storage` trait, and produces the authoritative on-disk bundle format that Phase 26 restore consumes — a cross-phase migration artefact. The substitute-and-re-pack determinism contract (`bundle-format-v1.md` §10) is a convention later phases rely on.
**Affected seams:** PROPOSED: bundle-packager-storage — "Bundle Packager ↔ Storage" — `trilithon_adapters::export::bundle_packager::export_bundle` reads snapshots and audit rows through `trilithon_core::storage::Storage`; the bundle format is the contract consumed by Phase 26 restore.
**Planned contract additions:** `trilithon_core::export::bundle::{render_core_parts, BundleInputs, CoreBundleParts}`, `trilithon_adapters::export::bundle_packager::{export_bundle, BundleExportRequest, BundleExportError}`
**Confidence:** high
**If low confidence, why:** n/a

### 25.8: Audit kinds plus artefact SHA-256 persistence
**Proposed tag:** [cross-cutting]
**Reasoning:** Adds three new `AuditEvent` variants to `core::audit` whose `Display` strings must match the architecture §6.6 audit vocabulary exactly — modifying the shared audit-event enum is an established cross-phase convention (every phase emitting an audit kind extends this enum and the §6.6 table in the same commit). The audit-kind ↔ wire-`kind` mapping is consumed by the storage adapter's insert-time validation, so the change ripples across the audit seam. Also wires the SHA-256-of-artefact note shape that 25.9 handlers must produce.
**Affected seams:** none active match; relates to the existing `applier-audit-writer` audit-event surface (`trilithon_core::audit::AuditEvent`) but does not exercise that apply-path seam directly.
**Planned contract additions:** `trilithon_core::audit::AuditEvent::{ExportCaddyJson, ExportCaddyfile, ExportBundle}`, `trilithon_core::audit::{ExportNotes, ExportFormat}`
**Confidence:** medium
**If low confidence, why:** The change is small in lines, but extending the shared `AuditEvent` enum plus the §6.6 vocabulary is a convention-bearing edit other phases follow, which is what pushes it to [cross-cutting].

### 25.9: HTTP export endpoints (three formats plus warnings sidecar)
**Proposed tag:** [cross-cutting]
**Reasoning:** Adds five HTTP handlers in `cli` that wire `core` exporters and the `adapters` bundle packager together — the handler reaches across all three layers, calling `trilithon_core::export::*`, `trilithon_adapters::export::bundle_packager`, and `Storage`. It registers new routes, emits `http.request.*` tracing events and three audit kinds, and references ADR-0009, PRD T2.9, architecture §6.6 and §12.1, and hazards H7/H10 — well past the 3+ ADR/PRD threshold. Clearly [cross-cutting].
**Affected seams:** none active match; integration point with the audit writer mirrors the `applier-audit-writer` pattern.
**Planned contract additions:** `trilithon_cli::http::export::{get_caddy_json, get_caddyfile, get_caddyfile_warnings, get_bundle, post_bundle, ExportHttpError, PostBundleBody}`
**Confidence:** high
**If low confidence, why:** n/a

### 25.10: CLI `trilithon export` subcommand
**Proposed tag:** [cross-cutting]
**Reasoning:** Adds a `clap` subcommand in `cli` that runs the export pipeline either over the loopback daemon or in-process directly against the local SQLite database — the in-process path crosses into `adapters` storage and must itself write the audit row via the `Storage` trait so the audit record exists regardless of the path taken. That dual-path behaviour spans the cli↔adapters boundary and replicates an audit convention, beyond a single-layer [standard] slice.
**Affected seams:** none
**Planned contract additions:** `trilithon_cli::commands::export::{ExportArgs, ExportCliFormat, run}`
**Confidence:** medium
**If low confidence, why:** If the in-process fallback were dropped and the CLI only called the daemon, this would be [standard]; the plan explicitly mandates the in-process path, so it crosses layers.

### 25.11: Web UI `ExportPanel`
**Proposed tag:** [standard]
**Reasoning:** Adds a single React component, its Vitest test, and a barrel file under `web/src/features/export/` — one module group in the `web` codebase, no shared abstraction, no backend or cross-layer change. The component takes callbacks as props and emits no audit/tracing events itself. Self-contained to one frontend feature, matching [standard].
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 25.12: Caddyfile round-trip integration test against Phase 13 corpus
**Proposed tag:** [standard]
**Reasoning:** Adds one integration test file in `adapters/tests/` that composes the 25.2 and 25.3 exporters with the Phase 13 parser; it is test-only code in a single crate's test directory and introduces no production code, no traits, and no cross-layer production dependency. Exercising multiple components from a test harness does not make the slice itself cross-cutting.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 25.13: Migration documentation page
**Proposed tag:** [trivial]
**Reasoning:** Adds one Markdown page and one heading-lint shell script — pure documentation, no code, no crate, no trait, no I/O, no audit/tracing event. Nothing in the build or other slices depends on its symbols. Squarely [trivial].
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

## Summary
- 1 trivial / 8 standard / 4 cross-cutting / 0 low-confidence

(Three slices — 25.3, 25.8, 25.10 — carry medium confidence; none are low.)

## Notes

- The contract registry (`contracts.md`) and `contract-roots.toml` currently
  hold only Phase 7 apply-path roots. Phase 25 introduces a substantial new
  public surface under `core::export`, `adapters::export`, and
  `cli::http::export`. The "Planned contract additions" listed above should be
  proposed into `contract-roots.toml` during `/phase-merge-review`; they are
  not yet ratified contracts.
- One new seam is proposed: **bundle-packager-storage**. Slice 25.7's
  `export_bundle` reads snapshots and audit rows through the `core::storage::Storage`
  trait, and the resulting bundle format is the explicit contract consumed by
  Phase 26's restore path (`bundle-format-v1.md` §1, §11). Per `seams.md` rules,
  `/tag-phase` cannot ratify a seam — this proposal goes to `seams-proposed.md`
  for `/phase-merge-review` to ratify.
- Slices 25.8 and 25.9 both write audit rows; they integrate with the shared
  `core::audit::AuditEvent` surface that the existing `applier-audit-writer`
  seam also touches, but neither slice exercises that apply-path seam's
  contracts directly, so no existing seam is claimed.
- The phase has a clean dependency spine: 25.1/25.4/25.5/25.6 are independent
  leaves; 25.7 fans them in; 25.8/25.9 wire audit + HTTP; 25.10/25.11 are
  surfaces; 25.12/25.13 are verification and docs. The four [cross-cutting]
  slices (25.7–25.10) form the integration core and warrant the closest review.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.

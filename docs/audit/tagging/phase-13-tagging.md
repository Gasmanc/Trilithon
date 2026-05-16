# Phase 13 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md (project instructions)
- docs/architecture/architecture.md (1124 lines)
- docs/architecture/trait-signatures.md (735 lines)
- docs/planning/PRD.md (953 lines)
- docs/adr/ — ADR-0001, ADR-0002, ADR-0009 read in full; ADR-0003..0016 indexed (16 ADRs present)
- docs/todo/phase-13-caddyfile-import.md (1574 lines, 13 slices)
- docs/architecture/seams.md (5 active seams, all Phase 7)
- docs/architecture/seams-proposed.md (empty staging)
- docs/architecture/contract-roots.toml (Phase 7 reconciler roots only)
- docs/architecture/contracts.md (empty registry)
**Slices analysed:** 13

---

## Proposed Tags

### 13.1: Lexer — token types and `lex` function
**Proposed tag:** [standard]
**Reasoning:** All files in one crate (`core/crates/core/src/caddyfile/lexer.rs` plus module registration in `mod.rs`/`lib.rs`). Introduces new public types (`Token`, `Span`, `Spanned`, `LexError`) and the `lex` function, but no trait and no cross-layer dependency — `core` stays pure with `#[forbid(unsafe_code)]`, no I/O, no async. It is the foundation other Phase 13 slices structurally depend on, but a foundational module addition within one crate is the canonical "standard" shape, not "cross-cutting": no shared trait is modified and no tracing/audit convention is introduced.
**Affected seams:** none
**Planned contract additions:** none (no `// contract:` markers; not in contract-roots.toml; `core::caddyfile` is internal API consumed only within Phase 13)
**Confidence:** high
**If low confidence, why:** n/a

### 13.2: Lexer fuzz harness
**Proposed tag:** [cross-cutting]
**Reasoning:** The fuzz target itself is trivial, but the slice creates a new `cargo fuzz` subcrate (`core/crates/core/fuzz/Cargo.toml`) and modifies CI workflow files (`.github/workflows/fuzz.yml`). A new workspace member plus a CI job is infrastructure that crosses the crate/build boundary and establishes a convention (a pinned `nightly-2026-04-01` fuzz job) that future fuzz targets follow. The CI-pipeline edit takes it outside a single crate's source tree.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** Arguable as [standard] if the new fuzz subcrate is treated as "tightly related second crate"; the CI workflow edit and the new-workspace-member convention tip it to cross-cutting.

### 13.3: Parser AST types and recursive-descent `parse`
**Proposed tag:** [standard]
**Reasoning:** Single crate (`core/crates/core/src/caddyfile/ast.rs`, `parser.rs`, `mod.rs`). Adds AST node types, `ParseOptions`, `ParseError`, and the `parse` function. No trait introduced or modified; `core` purity preserved; no I/O. The AST is consumed only by later Phase 13 slices — internal API, local to the slice. Non-trivial in size (10h, recursive-descent grammar) but self-contained, which is exactly [standard].
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 13.4: Snippet expander, env resolver, file-import resolver
**Proposed tag:** [cross-cutting]
**Reasoning:** `expand` and the `LossyWarning` provisional enum are pure `core`, but `resolve_file_imports` performs filesystem reads (`std::path::Path::canonicalize`, reading imported files). Architecture §4.1 forbids `core` from filesystem access in non-test code. This slice introduces real I/O into a module that the layer rules say must be pure — that is either a genuine layer-boundary tension that must be resolved (the file read belongs in `adapters`, or behind a trait like `EnvProvider`) or a design question the implementer must stop and raise. `resolve_env` correctly goes through the `EnvProvider` trait; `resolve_file_imports` has no equivalent abstraction in the slice. The I/O-into-`core` concern alone makes this cross-cutting.
**Affected seams:** none (no existing seam covers Caddyfile import; if the file-import I/O is pushed to an adapter, a PROPOSED seam for the core↔adapters import boundary may be warranted)
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** The slice text places `resolve_file_imports` in `core` with direct `canonicalize`/file reads, contradicting §4.1; the tag hinges on that boundary tension, which the implementer may resolve differently than the slice as written.

### 13.5: Size guards (input, directives, nesting, expansion factor, route count)
**Proposed tag:** [standard]
**Reasoning:** New `core/crates/core/src/caddyfile/limits.rs` plus counter wiring into `parser.rs` and `translator/mod.rs` — all within `core/crates/core`. Adds `SizeOptions`, `SizeError`, `check_input_size`. No trait, no cross-layer dependency, no I/O (constant-time length comparison and integer counters). Touches three files but one crate and one layer; the size-bound logic is a self-contained concern (hazard H15). Standard.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 13.6: `LossyWarning` catalogue and translator dispatch skeleton
**Proposed tag:** [standard]
**Reasoning:** Single crate (`core/crates/core/src/caddyfile/lossy.rs`, `translator/mod.rs`, `translator/dispatch.rs`). Defines the closed `LossyWarning` enum, `LossyWarningSet`, `TranslateContext`, `TranslateResult`, `TranslateError`, and the `translate` orchestration. The dispatch table is a within-module convention that slices 13.7/13.8 plug into — not a shared trait and not a tracing/audit convention spanning phases. The `LossyWarning` kebab-case ID set is later mirrored in TypeScript (slice 13.13) but that mirroring is owned by 13.13, not introduced as a cross-phase contract here. No trait, no I/O, no layer crossing.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** The `LossyWarning` enum becomes a de-facto cross-slice contract (13.7, 13.8, 13.12, 13.13 all depend on its variants/IDs); if treated as an introduced shared convention it edges toward cross-cutting, but it is confined to the Caddyfile module and one downstream phase, not a workspace-wide convention.

### 13.7: Translator batch A — sites, `reverse_proxy`, `file_server`, `redir`, `respond`
**Proposed tag:** [standard]
**Reasoning:** Five handler files all under `core/crates/core/src/caddyfile/translator/`. Each handler is a `pub fn` plugging into the 13.6 dispatch table. Consumes the existing mutation algebra (`CreateRoute`, `SetUpstream`, etc. from Phase 4) but does not modify it or any trait. `core` purity preserved, no I/O. Large (12h) but self-contained translation logic in one crate/module. Standard.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 13.8: Translator batch B — handlers, headers, TLS, encode, log, matchers
**Proposed tag:** [standard]
**Reasoning:** Six handler files under `core/crates/core/src/caddyfile/translator/`. Same shape as 13.7 — `pub fn` handlers on the dispatch table. The TLS handler reads `ctx.capabilities` (the in-memory `CapabilitySet` already threaded through `TranslateContext`), which is a parameter read, not a new cross-layer dependency or I/O. No trait introduced or modified. One crate, one layer. Standard.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 13.9: Fixture corpus authoring batches `01_trivial` through `06_snippets`
**Proposed tag:** [standard]
**Reasoning:** Test-only: a corpus runner (`tests/caddyfile_corpus.rs`) and fixture directories under `core/crates/core/tests/`. All within `core/crates/core`, no production code, no trait, no layer crossing. Golden generation needs the Caddy 2.11.2 binary (an external tool at fixture-authoring time), but the committed artefacts and the test runner are pure in-crate test code. The `insta` golden test pattern is already established by earlier phases — no new convention. Standard.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 13.10: Fixture corpus authoring batches `07_imports` through `11_pathological`
**Proposed tag:** [standard]
**Reasoning:** Test-only: remaining fixture batches plus two integration test files (`caddyfile_pathological.rs`, `caddyfile_lossy_completeness.rs`) under `core/crates/core/tests/`. The pathological tests sample resident memory via `getrusage`, but that is test-harness measurement, not production I/O or a new trait. One crate, one layer, no production code. Standard.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 13.11: Round-trip equivalence harness and normalisation rules
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span two crates: `core/crates/core/src/caddyfile/normalise.rs` (production code in `core`) and `core/crates/adapters/tests/caddyfile_round_trip.rs` plus `adapters/tests/helpers/caddy_runner.rs` (test code in `adapters`). The harness boots a real Caddy subprocess and replays HTTP requests — process and network I/O — and it integrates the Phase 4/Phase 7 `DesiredState`→Caddy-JSON renderer, exercising the existing `applier-caddy-admin` seam substrate. Multi-crate span plus a cross-component integration harness that depends structurally on prior phases' renderer output makes this cross-cutting.
**Affected seams:** `applier-caddy-admin` (exercised indirectly — the round-trip harness validates the same `DesiredState`→Caddy JSON substrate; no contract change, validation only)
**Planned contract additions:** none (`normalise` functions are internal Caddyfile-module helpers, not contract roots)
**Confidence:** medium
**If low confidence, why:** The cross-crate split is test-helper code (adapters tests consuming a core module); if test-only multi-crate placement is discounted, it reads as [standard], but the live-Caddy harness and renderer dependency keep it cross-cutting.

### 13.12: `ImportFromCaddyfile` mutation, HTTP endpoints, audit row authoring
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span three crates and cross two layer boundaries: `core/crates/core/src/mutation.rs` and `audit.rs` (core), `core/crates/cli/src/http/imports.rs` (cli). It extends the shared `TypedMutation` enum (consumed by the web UI and tool gateway per T1.6 — "the single API"), adds a new `AuditEvent::ImportCaddyfile` variant emitting the `import.caddyfile` kind (architecture §6.6 audit vocabulary, which must be updated in the same commit), and emits tracing events (`http.request.received/completed`, `apply.started/succeeded/failed`). It wires through the Phase 7 applier and Phase 5 snapshot writer. Adding an audit kind, extending a shared mutation enum, and crossing core→cli are each independently cross-cutting triggers; references T1.5/T1.6/T1.7 + ADR-0009 (3+ PRD/ADR refs).
**Affected seams:** `applier-caddy-admin`, `applier-audit-writer` (the import apply path drives both — the audit row obligation for the import is the `applier-audit-writer` seam; no contract change, the existing `AuditEvent` contract gains a variant)
**Planned contract additions:** none required as new roots, but `core::audit::AuditEvent` gains the `ImportCaddyfile` variant and `core::mutation::TypedMutation` gains `ImportFromCaddyfile` — if either is later promoted to a contract root, that is a separate `/phase-merge-review` action. Architecture §6.6 audit `kind` table and §12.1 are authoritative and must be updated in-commit (the `import.caddyfile` kind already exists in §6.6).
**Confidence:** high
**If low confidence, why:** n/a

### 13.13: Web UI — Import wizard, `LossyWarningList`, `MutationPreviewList`
**Proposed tag:** [standard]
**Reasoning:** All files under `web/src/` — one frontend package, the `caddyfile-import` feature plus one shared component. React/TypeScript, no Rust trait, no Rust layer crossing. It mirrors the `LossyWarning` ID union in TypeScript and is built for reuse by Phase 25's export panel, but exporting a component for later reuse is normal frontend structure, not an introduced cross-cutting convention. It consumes the 13.12 HTTP endpoints over the wire — a network call, not a code-level layer dependency. Self-contained feature work in one package: standard.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

---

## Summary
- 8 trivial: 0
- 8 standard: 13.1, 13.3, 13.5, 13.6, 13.7, 13.8, 13.9, 13.10, 13.13 (9)
- 4 cross-cutting: 13.2, 13.4, 13.11, 13.12 (4)
- low-confidence: 13.2, 13.4, 13.6, 13.11 (4 medium-confidence)

Corrected counts: 0 trivial, 9 standard, 4 cross-cutting, 4 medium-confidence.

## Notes

- **No [trivial] slices.** Every slice in Phase 13 adds new public types or functions and is at minimum a self-contained module — none is a pure enum addition or single-helper change. This is expected for a feature phase that builds a whole subsystem (the Caddyfile parser/translator pipeline) from scratch.

- **`core` purity tension at 13.4.** The slice as written places `resolve_file_imports` (filesystem `canonicalize` + file reads) inside `core/crates/core/src/caddyfile/import.rs`. Architecture §4.1 and §5 forbid `core` from filesystem access in non-test code. The implementer should stop and confirm: either the file-import resolution moves to `adapters` (or behind a `core` trait analogous to `EnvProvider`), or the slice's file placement is wrong. This is the single most important finding in this analysis — it is a layer-rule violation as drafted, and the [cross-cutting] tag flags it for the deeper review `/phase` gives cross-cutting slices.

- **No seam additions proposed.** `seams.md` contains only Phase 7 apply-path seams; `seams-proposed.md` is empty. Phase 13's import path is a new boundary, but it rides entirely on existing Phase 7 seams (`applier-caddy-admin`, `applier-audit-writer`) — the import is just another mutation through the established applier. No genuinely new architectural boundary is introduced, so nothing is staged to `seams-proposed.md`. If 13.4's file-import I/O is relocated to an adapter, a core↔adapters import-resolver boundary could merit a proposed seam — flagged for `/phase-merge-review`.

- **No contract registry impact.** `contracts.md` is empty and `contract-roots.toml` lists only Phase 7 reconciler symbols. Phase 13 adds public API (`core::caddyfile::*`, `TypedMutation::ImportFromCaddyfile`, `AuditEvent::ImportCaddyfile`) but the slice files carry no `// contract:` markers and the TODO does not direct adding roots. Treated as internal API. Promotion to contract roots, if desired, is a separate human-curated `/phase-merge-review` action.

- **Audit/tracing vocabulary (13.12).** `import.caddyfile` already exists in architecture §6.6; the `AuditEvent::ImportCaddyfile` Rust variant and its `Display` mapping must land in the same commit (§6.6 "Rust `AuditEvent` ↔ wire `kind` mapping" already lists the row). Tracing events emitted (`http.request.*`, `apply.*`) are all already in the §12.1 closed vocabulary — no new event names, so no §12.1 edit required. This is why 13.12 is cross-cutting on the audit-convention trigger even though it introduces no *new* vocabulary: it is the slice that wires a phase-spanning audit kind into emission.

- **13.2 build-graph impact.** Adding `core/crates/core/fuzz` as a workspace member and a CI job is the reason 13.2 is cross-cutting despite a 3-line fuzz target. The workspace manifest and CI pipeline are shared infrastructure; the tag reflects the blast radius, not the line count.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Modifications (if any)
None.

### Notes from user
Auto-accepted.

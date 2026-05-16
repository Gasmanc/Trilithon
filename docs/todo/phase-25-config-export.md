# Phase 25 — Configuration Export (JSON, Caddyfile, Native Bundle) — Implementation Slices

> Phase reference: [../phases/phase-25-config-export.md](../phases/phase-25-config-export.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Bundle format (authoritative): [bundle-format-v1.md](../architecture/bundle-format-v1.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference (`docs/phases/phase-25-config-export.md`).
- `docs/architecture/bundle-format-v1.md` — authoritative spec for every byte of the native bundle. Implementations MUST track this document; any deviation is an implementation bug.
- Architecture §6.5 (snapshots), §6.6 (audit log kinds), §6.9 (secrets metadata), §11 (security posture), §12.1 (tracing vocabulary), §14 (upgrade and migration).
- Trait signatures: `core::storage::Storage` (snapshot and audit-log access for the bundle packager), `core::secrets::SecretsVault` (master-key access for the wrap).
- ADRs: ADR-0009 (immutable content-addressed snapshots and audit log), ADR-0014 (secrets vault — bundle wraps the master key).
- PRD: T2.9 (configuration export).
- Hazards: H7 (Caddyfile escape lock-in — this phase mitigates), H10 (secrets in audit diffs — applies to the bundle's audit-log member).

## Slice plan summary

| # | Title | Primary files | Effort (ideal-eng-hours) | Depends on |
|---|-------|---------------|--------------------------|------------|
| 25.1 | Deterministic JSON-ordering helper | `core/crates/core/src/export/deterministic.rs` | 4 | — |
| 25.2 | Caddy JSON serialiser and integration test | `core/crates/core/src/export/caddy_json.rs`, `core/crates/adapters/tests/export_caddy_json_round_trip.rs` | 6 | 25.1 |
| 25.3 | Caddyfile printer with snippet deduplication and translation reference | `core/crates/core/src/caddyfile/printer.rs`, `core/crates/core/src/caddyfile/printer/snippets.rs`, `docs/architecture/caddyfile-translation.md` | 10 | 25.1 |
| 25.4 | Bundle manifest schema (Rust type and JSON Schema) | `core/crates/core/src/export/manifest.rs`, `docs/schemas/bundle-manifest.json` | 4 | — |
| 25.5 | Deterministic tar packer | `core/crates/adapters/src/export/tar_packer.rs` | 5 | — |
| 25.6 | Master-key wrap (Argon2id + XChaCha20-Poly1305) | `core/crates/adapters/src/export/master_key_wrap.rs` | 5 | — |
| 25.7 | Bundle exporter and named determinism test | `core/crates/core/src/export/bundle.rs`, `core/crates/adapters/src/export/bundle_packager.rs`, `core/crates/adapters/src/export/bundle/tests.rs` | 8 | 25.1, 25.4, 25.5, 25.6 |
| 25.8 | Audit kinds plus artefact SHA-256 persistence | `core/crates/core/src/audit.rs`, `core/crates/cli/src/http/export.rs` (audit hook) | 4 | 25.2, 25.3, 25.7 |
| 25.9 | HTTP export endpoints (three formats plus warnings sidecar) | `core/crates/cli/src/http/export.rs` | 6 | 25.2, 25.3, 25.7, 25.8 |
| 25.10 | CLI `trilithon export` subcommand | `core/crates/cli/src/commands/export.rs` | 4 | 25.9 |
| 25.11 | Web UI `ExportPanel` | `web/src/features/export/ExportPanel.tsx`, `web/src/features/export/ExportPanel.test.tsx` | 5 | 25.9 |
| 25.12 | Caddyfile round-trip integration test against Phase 13 corpus | `core/crates/adapters/tests/export_caddyfile_round_trip.rs` | 4 | 25.3 |
| 25.13 | Migration documentation page | `docs/migrating-off-trilithon.md` | 2 | 25.9 |

---

## Slice 25.1 [standard] — Deterministic JSON-ordering helper

### Goal

A reusable canonical-JSON writer that the Caddy JSON exporter and the bundle packager both consume. Output MUST sort object keys lexicographically at every level, normalise number formatting, and produce byte-stable output for byte-equal inputs.

### Entry conditions

- The `core/crates/core` crate exists (Phase 1) with `serde` and `serde_json` as dependencies.

### Files to create or modify

- `core/crates/core/src/export/mod.rs` — the export module root.
- `core/crates/core/src/export/deterministic.rs` — the canonical-JSON writer.
- `core/crates/core/src/lib.rs` — register `pub mod export;`.

### Signatures and shapes

```rust
//! Canonical JSON serialisation for deterministic export.
//!
//! Per architecture §11 and bundle-format-v1.md §4, every JSON
//! artefact Trilithon exports MUST have lexicographically-sorted
//! object keys at every level. Number formatting follows
//! `serde_json`'s default (no scientific notation for integers, no
//! trailing zeros on floats). This module is the single boundary at
//! which the ordering rule is enforced.

use std::io::Write;

use serde_json::Value;

/// Errors produced by the canonical writer.
#[derive(Debug, thiserror::Error)]
pub enum CanonicalJsonError {
    #[error("serde_json error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Serialise `value` as canonical JSON to `writer`. Object keys are
/// sorted lexicographically at every level. Output is pretty-printed
/// with two-space indentation.
pub fn write_canonical_pretty<W: Write>(
    writer: &mut W,
    value: &Value,
) -> Result<(), CanonicalJsonError> {
    let canonical = canonicalise(value);
    serde_json::to_writer_pretty(writer, &canonical)?;
    Ok(())
}

/// Serialise `value` as canonical JSON without indentation. Used for
/// content-addressed payloads where a stable, compact form is required.
pub fn write_canonical_compact<W: Write>(
    writer: &mut W,
    value: &Value,
) -> Result<(), CanonicalJsonError> {
    let canonical = canonicalise(value);
    serde_json::to_writer(writer, &canonical)?;
    Ok(())
}

/// Recursively reconstruct `value` so every `Value::Object` is a
/// `serde_json::Map` whose keys are inserted in lexicographic order.
fn canonicalise(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::with_capacity(map.len());
            for key in keys {
                if let Some(child) = map.get(key) {
                    out.insert(key.clone(), canonicalise(child));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(canonicalise).collect())
        }
        other => other.clone(),
    }
}
```

### Algorithm

1. Walk the input `serde_json::Value` recursively.
2. For each object, collect the keys, sort them lexicographically (default `String` `Ord`), reconstruct the map in sorted order.
3. For each array, preserve element order (arrays in Caddy JSON carry semantic order; route ordering is significant).
4. Pass the canonicalised value to `serde_json::to_writer_pretty` (or `to_writer` for compact form).

### Tests

- `export::deterministic::tests::object_keys_sorted` — input `{ "z": 1, "a": 2 }` produces `{ "a": 2, "z": 1 }` byte-stably.
- `export::deterministic::tests::nested_objects_sorted_at_every_level` — input `{ "a": { "z": 1, "y": 2 } }` produces `{ "a": { "y": 2, "z": 1 } }`.
- `export::deterministic::tests::arrays_preserve_order` — input `[3, 1, 2]` produces `[3, 1, 2]`.
- `export::deterministic::tests::byte_stable_across_calls` — write the same `Value` twice into separate `Vec<u8>`; assert `vec1 == vec2`.

### Acceptance command

```
cargo test -p trilithon-core export::deterministic
```

### Exit conditions

- The four named tests pass.
- The module is `pub use`d from `core::export`.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0009.
- Architecture §11.
- `bundle-format-v1.md` §4.

---

## Slice 25.2 [standard] — Caddy JSON serialiser and integration test

### Goal

`export_caddy_json` produces a byte-deterministic Caddy JSON document equal to what Trilithon would `POST /load`. Trilithon-specific extensions (`@id` annotations) are stripped. Output, written to a fresh Caddy via `caddy run --config`, produces identical runtime behaviour against the Phase 13 request matrix.

### Entry conditions

- Slice 25.1 complete.
- The `DesiredState` type and its conversion to Caddy JSON exists from Phase 4.

### Files to create or modify

- `core/crates/core/src/export/caddy_json.rs` — the exporter.
- `core/crates/adapters/tests/export_caddy_json_round_trip.rs` — fresh-Caddy integration test.

### Signatures and shapes

```rust
//! Caddy JSON exporter.
//!
//! Produces a byte-deterministic Caddy JSON document equal to what
//! Trilithon would POST to /load. Trilithon-specific extensions
//! (`@id` annotations) are stripped at this boundary so the output is
//! consumable by stock Caddy.

use crate::export::deterministic::write_canonical_pretty;
use crate::DesiredState;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("serialisation failed: {0}")]
    Serialisation(#[from] crate::export::deterministic::CanonicalJsonError),
    #[error("internal: failed to render desired state: {0}")]
    Render(String),
}

/// Export `state` as canonical Caddy JSON. The output:
///
/// - has lexicographically-sorted object keys at every level;
/// - uses two-space indentation (`serde_json::to_vec_pretty`);
/// - strips every `@id` annotation Trilithon adds for ownership tracking;
/// - is byte-equal across invocations on byte-equal `state`.
pub fn export_caddy_json(state: &DesiredState) -> Result<Vec<u8>, ExportError> {
    let mut value = state.render_caddy_json()
        .map_err(|err| ExportError::Render(err.to_string()))?;
    strip_trilithon_extensions(&mut value);
    let mut buffer = Vec::with_capacity(8 * 1024);
    write_canonical_pretty(&mut buffer, &value)?;
    Ok(buffer)
}

/// Recursively remove every `"@id"` key from objects.
fn strip_trilithon_extensions(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("@id");
            for (_, child) in map.iter_mut() {
                strip_trilithon_extensions(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                strip_trilithon_extensions(item);
            }
        }
        _ => {}
    }
}
```

### Algorithm

1. Render `DesiredState` to its `serde_json::Value` representation via the existing `render_caddy_json` method (Phase 4).
2. Recursively strip every `"@id"` object key.
3. Write the value through `write_canonical_pretty` into a `Vec<u8>`.

### Tests

- `export::caddy_json::tests::deterministic_output` — call `export_caddy_json` twice on the same state; assert byte equality.
- `export::caddy_json::tests::strips_trilithon_id_annotations` — input state with `@id` annotations; assert none appear in the output.
- `export_caddy_json_round_trip.rs` (integration) — for every non-pathological fixture in the Phase 13 corpus, render the output to a temp file, run `caddy run --config <file>` in a child process, run the Phase 13 request matrix, assert responses byte-match those produced by the source instance.

### Acceptance command

```
cargo test -p trilithon-core export::caddy_json \
  && cargo test -p trilithon-adapters --test export_caddy_json_round_trip
```

### Exit conditions

- `export_caddy_json` exists with the signature above.
- Determinism test passes.
- Integration test passes for every non-pathological fixture in the Phase 13 corpus.

### Audit kinds emitted

`export.caddy-json` (architecture §6.6) — wired in slice 25.8.

### Tracing events emitted

`http.request.received`, `http.request.completed` (architecture §12.1) — at the HTTP handler boundary in slice 25.9.

### Cross-references

- ADR-0009.
- PRD T2.9.
- Architecture §11.

---

## Slice 25.3 [standard] — Caddyfile printer with snippet deduplication and translation reference

### Goal

A Caddyfile printer renders `DesiredState` to legible Caddyfile syntax, with a leading comment block, snippet deduplication for header sets that appear on more than one route, and a structured list of `LossyWarning`s for constructs that cannot translate cleanly. A translation-reference document at `docs/architecture/caddyfile-translation.md` lists every Trilithon construct and its Caddyfile mapping marked clean / lossy / unsupported.

### Entry conditions

- Phase 13's grammar definitions and parser are available.
- Slice 25.1 complete (deterministic helper used for any embedded JSON).

### Files to create or modify

- `core/crates/core/src/caddyfile/printer.rs` — the printer.
- `core/crates/core/src/caddyfile/printer/snippets.rs` — snippet deduplication helper.
- `docs/architecture/caddyfile-translation.md` — translation reference.

### Signatures and shapes

```rust
//! Caddyfile printer (best-effort lossy export).
//!
//! Phase 25 / T2.9 / mitigates H7. The output is human-readable
//! Caddyfile syntax with a leading comment block, snippet
//! deduplication, and a structured list of lossy warnings for
//! constructs that cannot translate cleanly. The authoritative
//! configuration is the native bundle (Phase 25 / bundle-format-v1.md);
//! the Caddyfile is for users walking away.

use crate::DesiredState;

/// One construct in `DesiredState` that does not translate cleanly to
/// Caddyfile. Identifiers MUST be stable across releases so that the
/// `docs/architecture/caddyfile-translation.md` lint can verify every
/// stable identifier has a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LossyWarning {
    /// Generic export loss with a free-text note.
    CaddyfileExportLoss {
        construct: String,
        route_id: Option<String>,
        note: String,
    },
}

/// The full result of a Caddyfile print.
#[derive(Debug, Clone)]
pub struct PrintResult {
    /// The Caddyfile body.
    pub caddyfile: String,
    /// Structured warnings.
    pub warnings: Vec<LossyWarning>,
    /// The `<filename>.warnings.txt` sidecar text.
    pub sidecar_warnings_text: String,
}

/// Print `state` as Caddyfile syntax. The output begins with the
/// leading comment block:
///
/// ```text
/// # Generated by Trilithon vX.Y.Z on <UTC timestamp>
/// # Source snapshot: <snapshot-hash>
/// # WARNING: this Caddyfile is a best-effort rendering. The authoritative
/// # configuration is the Trilithon native bundle. See sidecar warnings file.
/// ```
///
/// Snippet deduplication threshold: header sets appearing on two or
/// more routes are extracted as named snippets and `import`-ed.
pub fn print(state: &DesiredState) -> PrintResult {
    let snippet_set = snippets::extract_snippets(state.routes());
    let header = leading_comment_block(state);
    let mut body = String::new();
    body.push_str(&header);
    body.push_str(&snippet_set.render());
    let mut warnings = Vec::new();
    body.push_str(&render_routes(state, &snippet_set, &mut warnings));
    PrintResult {
        sidecar_warnings_text: render_sidecar(&warnings),
        caddyfile: body,
        warnings,
    }
}

fn leading_comment_block(state: &DesiredState) -> String {
    format!(
        "# Generated by Trilithon v{} on {}\n\
         # Source snapshot: {}\n\
         # WARNING: this Caddyfile is a best-effort rendering. The authoritative\n\
         # configuration is the Trilithon native bundle. See sidecar warnings file.\n\n",
        env!("CARGO_PKG_VERSION"),
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        state.current_snapshot_id().unwrap_or_else(|| "unknown".to_string()),
    )
}

fn render_sidecar(warnings: &[LossyWarning]) -> String {
    let mut out = String::new();
    out.push_str("Trilithon Caddyfile export — lossy warnings\n\n");
    for warning in warnings {
        match warning {
            LossyWarning::CaddyfileExportLoss { construct, route_id, note } => {
                out.push_str(&format!(
                    "- [{}] route={} — {}\n",
                    construct,
                    route_id.as_deref().unwrap_or("(global)"),
                    note,
                ));
            }
        }
    }
    out
}

fn render_routes(
    state: &DesiredState,
    snippets: &snippets::SnippetSet,
    warnings: &mut Vec<LossyWarning>,
) -> String {
    // Implementation walks the route list; emits site blocks, applies
    // snippet imports, accumulates LossyWarnings for forward-auth
    // attached via the secrets vault, named-matcher composition that
    // exceeds Caddyfile expressivity, and custom rate-limit bucket
    // keys (per phase reference).
    todo!("implementation per docs/architecture/caddyfile-translation.md")
}
```

```rust
//! Snippet deduplication helper.
//!
//! Header sets that appear on two or more routes are extracted as
//! Caddyfile snippets and `import`-ed by each route that uses them.
//! Threshold: two appearances. One appearance does NOT trigger
//! extraction.

use std::collections::HashMap;

use crate::routes::Route;

/// A single snippet: a name plus its body lines.
#[derive(Debug, Clone)]
pub struct Snippet {
    pub name: String,
    pub body_lines: Vec<String>,
}

/// The full set of snippets extracted from a route list.
#[derive(Debug, Default)]
pub struct SnippetSet {
    pub snippets: Vec<Snippet>,
    /// Map from route id to the list of snippet names it imports.
    pub imports_by_route: HashMap<String, Vec<String>>,
}

impl SnippetSet {
    /// Render the snippet definitions to a Caddyfile preamble.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for snippet in &self.snippets {
            out.push_str(&format!("(snippet-{}) {{\n", snippet.name));
            for line in &snippet.body_lines {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
            out.push_str("}\n\n");
        }
        out
    }
}

/// Extract reusable snippets. A header set is reusable iff it appears
/// on at least two routes.
pub fn extract_snippets(routes: &[Route]) -> SnippetSet {
    // Implementation: hash each route's header set; group routes by
    // their header-set hash; emit a snippet for every group with size >= 2.
    todo!("implementation per phase reference")
}
```

`docs/architecture/caddyfile-translation.md` outline:

```markdown
# Caddyfile translation reference

This page is a field-by-field table of every Trilithon construct and
its Caddyfile mapping, marked clean / lossy / unsupported. Every
`LossyWarning::CaddyfileExportLoss { construct: <id> }` value MUST
have a row here. A documentation lint enforces this invariant.

| Construct (stable id) | Caddyfile mapping | Status | Notes |
| --- | --- | --- | --- |
| `route.basic-reverse-proxy` | `reverse_proxy <upstream>` | clean | |
| `route.tls-host-binding` | site block address | clean | |
| `route.header-set` | `header { ... }` | clean | |
| `route.forward-auth-from-vault` | (none) | lossy | Caddyfile cannot reference Trilithon's secrets vault. Operator MUST replace with an environment-variable reference before applying. |
| `route.named-matcher-composition` | named matcher `@name` | lossy | Compositions exceeding two operands collapse to a single matcher with the operands inlined. |
| `route.custom-rate-limit-bucket-key` | (none) | unsupported | Caddy ratelimit module not addressable from Caddyfile in the V1 supported subset. |
```

### Algorithm

`extract_snippets`:

1. For each `Route`, compute a stable hash of its header set (sorted, normalised key/value pairs).
2. Group routes by hash.
3. For every group with `len >= 2`, generate a snippet name (for example `header-set-<short-hash>`), build a `Snippet`, record imports in `imports_by_route`.

`print`:

1. Build the leading comment block including Trilithon version, UTC timestamp, snapshot id.
2. Extract snippets.
3. Render the snippet preamble.
4. Walk routes; for each route render its site block, applying snippet imports where applicable. For each construct that cannot translate cleanly, push a `LossyWarning` and skip the construct in the rendered output.
5. Build the sidecar warnings text from the warnings.

### Tests

- `caddyfile::printer::tests::leading_comment_present` — output begins with the four mandated comment lines.
- `caddyfile::printer::tests::snippet_dedup_at_two_appearances` — two routes with identical header set produce a snippet; one route with the same set does not.
- `caddyfile::printer::tests::lossy_warning_for_vault_forward_auth` — a route with forward-auth from the vault produces `LossyWarning::CaddyfileExportLoss { construct: "route.forward-auth-from-vault", .. }`.
- `caddyfile::printer::snippets::tests::single_appearance_not_extracted` — one appearance does not trigger extraction.
- Documentation lint at `docs/architecture/test/lint-caddyfile-translation.sh` — for every stable `construct` id used by `LossyWarning::CaddyfileExportLoss`, assert the table contains a row whose first column matches.

### Acceptance command

```
cargo test -p trilithon-core caddyfile::printer \
  && bash docs/architecture/test/lint-caddyfile-translation.sh
```

### Exit conditions

- The printer renders the Caddyfile with the leading comment, snippet preamble, and route blocks.
- The snippet helper extracts at the two-appearance threshold.
- Every `LossyWarning::CaddyfileExportLoss` construct id has a row in the translation reference.
- The documentation lint passes.

### Audit kinds emitted

`export.caddyfile` (architecture §6.6) — wired in slice 25.8.

### Tracing events emitted

None at the printer layer.

### Cross-references

- ADR-0009.
- PRD T2.9 (mitigates H7).
- Hazards: H7.

---

## Slice 25.4 [standard] — Bundle manifest schema (Rust type and JSON Schema)

### Goal

A Rust `BundleManifest` type and a published JSON Schema document that together describe `manifest.json` per `bundle-format-v1.md` §4. Every field listed in the bundle-format spec MUST appear with the correct type and presence rules.

### Entry conditions

- `bundle-format-v1.md` is committed (it already exists).

### Files to create or modify

- `core/crates/core/src/export/manifest.rs` — Rust types.
- `docs/schemas/bundle-manifest.json` — JSON Schema (draft 2020-12).

### Signatures and shapes

```rust
//! Bundle manifest types.
//!
//! Authoritative spec: `docs/architecture/bundle-format-v1.md` §4.
//! This module's types MUST track that document; any divergence is
//! an implementation bug.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionPosture {
    /// Bundle includes encrypted secrets blob; manifest declares
    /// `secrets_included = true`.
    SecretsIncludedEncrypted,
    /// Bundle excludes the secrets blob entirely.
    SecretsExcluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretsEncryption {
    pub algorithm: String,         // "xchacha20poly1305"
    pub kdf: String,               // "argon2id"
    pub kdf_params: KdfParams,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_cost: u32,               // 65536
    pub t_cost: u32,               // 3
    pub p_cost: u32,               // 4
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub schema_version: u32,
    pub trilithon_version: String,
    pub caddy_version: String,
    pub exported_at: String,                  // RFC 3339 UTC
    pub root_snapshot_id: String,             // lowercase hex SHA-256
    pub source_installation_id: String,
    pub snapshot_count: u32,
    pub audit_event_count: u32,
    pub secrets_included: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets_encryption: Option<SecretsEncryption>,
    pub caddy_admin_endpoint_at_export: String,
    pub bundle_sha256: String,                // lowercase hex; placeholder during pack
    pub master_key_wrap_present: bool,
    pub redaction_posture: RedactionPosture,
}

impl BundleManifest {
    /// The 64-character ASCII-zero placeholder used during the
    /// substitute-and-re-pack step (bundle-format-v1.md §4).
    pub const BUNDLE_SHA256_PLACEHOLDER: &'static str =
        "0000000000000000000000000000000000000000000000000000000000000000";
}
```

`docs/schemas/bundle-manifest.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://example.invalid/trilithon/bundle-manifest-v1.json",
  "title": "Trilithon bundle manifest v1",
  "type": "object",
  "required": [
    "schema_version",
    "trilithon_version",
    "caddy_version",
    "exported_at",
    "root_snapshot_id",
    "source_installation_id",
    "snapshot_count",
    "audit_event_count",
    "secrets_included",
    "caddy_admin_endpoint_at_export",
    "bundle_sha256",
    "master_key_wrap_present",
    "redaction_posture"
  ],
  "properties": {
    "schema_version": { "const": 1 },
    "trilithon_version": { "type": "string", "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+" },
    "caddy_version": { "type": "string", "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+" },
    "exported_at": { "type": "string", "format": "date-time" },
    "root_snapshot_id": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
    "source_installation_id": { "type": "string", "minLength": 1 },
    "snapshot_count": { "type": "integer", "minimum": 0 },
    "audit_event_count": { "type": "integer", "minimum": 0 },
    "secrets_included": { "type": "boolean" },
    "secrets_encryption": {
      "type": "object",
      "required": ["algorithm", "kdf", "kdf_params"],
      "properties": {
        "algorithm": { "const": "xchacha20poly1305" },
        "kdf": { "const": "argon2id" },
        "kdf_params": {
          "type": "object",
          "required": ["m_cost", "t_cost", "p_cost"],
          "properties": {
            "m_cost": { "const": 65536 },
            "t_cost": { "const": 3 },
            "p_cost": { "const": 4 }
          }
        }
      }
    },
    "caddy_admin_endpoint_at_export": { "type": "string" },
    "bundle_sha256": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
    "master_key_wrap_present": { "type": "boolean" },
    "redaction_posture": {
      "type": "string",
      "enum": ["secrets_included_encrypted", "secrets_excluded"]
    }
  },
  "if": { "properties": { "secrets_included": { "const": true } } },
  "then": { "required": ["secrets_encryption"] }
}
```

### Algorithm

1. Define the Rust types per the spec.
2. Author the JSON Schema draft 2020-12.
3. Validation test: serialise a representative `BundleManifest`, validate it against the schema, assert success.

### Tests

- `export::manifest::tests::serialises_with_required_fields` — round-trips a manifest through `serde_json`.
- `export::manifest::tests::matches_schema` — uses `jsonschema` to validate against `docs/schemas/bundle-manifest.json`.
- `export::manifest::tests::missing_secrets_encryption_when_secrets_included_fails` — a manifest with `secrets_included = true` and no `secrets_encryption` MUST fail validation.

### Acceptance command

```
cargo test -p trilithon-core export::manifest
```

### Exit conditions

- The Rust types match every field in `bundle-format-v1.md` §4.
- The JSON Schema validates a known-good manifest and rejects the conditional violation.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- `bundle-format-v1.md` §4.
- ADR-0014.

---

## Slice 25.5 [standard] — Deterministic tar packer

### Goal

A tar packer that produces byte-identical archives for byte-identical inputs by enforcing the determinism rules from `bundle-format-v1.md` §2: zero mtimes, zero uid/gid, empty uname/gname, fixed modes, lexicographic member order, no PAX or global headers, gzip with no filename and zero mtime.

### Entry conditions

- Cargo dependencies `tar = "0.4"` and `flate2 = "1"` are added to `core/crates/adapters`.

### Files to create or modify

- `core/crates/adapters/src/export/mod.rs` — module root.
- `core/crates/adapters/src/export/tar_packer.rs` — the packer.

### Signatures and shapes

```rust
//! Deterministic tar packer.
//!
//! Authoritative spec: `docs/architecture/bundle-format-v1.md` §2.
//! Every entry: `mtime = 0`, `uid = 0`, `gid = 0`, `uname = ""`,
//! `gname = ""`, file mode `0644`, directory mode `0755`. No PAX
//! extended headers, no global headers. Members written in
//! lexicographic order. Gzip wrapper: no filename in the header,
//! mtime field zero, OS byte zero, level 6.

use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("duplicate member path: {path}")]
    DuplicatePath { path: String },
    #[error("invalid member path: {path}")]
    InvalidPath { path: String },
}

/// One member of the archive.
#[derive(Debug, Clone)]
pub struct TarMember {
    /// Lowercase POSIX path inside the archive.
    pub path: String,
    /// `false` for files, `true` for directories.
    pub is_directory: bool,
    /// Body bytes for files; empty for directories.
    pub body: Vec<u8>,
}

/// Pack `members` deterministically into `output`. The caller MUST
/// pass `output` as a `Write`; this function consumes the gzip-encoded
/// tar stream into it.
pub fn pack<W: Write>(
    mut members: Vec<TarMember>,
    output: W,
) -> Result<(), PackError> {
    members.sort_by(|a, b| a.path.cmp(&b.path));
    {
        let mut seen = std::collections::HashSet::new();
        for member in &members {
            if !seen.insert(member.path.clone()) {
                return Err(PackError::DuplicatePath { path: member.path.clone() });
            }
            if member.path.is_empty() || member.path.starts_with('/') {
                return Err(PackError::InvalidPath { path: member.path.clone() });
            }
        }
    }

    // Gzip with no filename, mtime zero, OS byte zero. flate2's
    // GzBuilder is the only API that exposes those fields.
    let gz = flate2::GzBuilder::new()
        .mtime(0)
        .operating_system(0)
        .write(output, Compression::new(6));
    let mut tar_writer = tar::Builder::new(gz);
    tar_writer.mode(tar::HeaderMode::Deterministic);

    for member in &members {
        let mut header = tar::Header::new_ustar();
        header.set_size(member.body.len() as u64);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_username("")
            .map_err(|e| PackError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        header.set_groupname("")
            .map_err(|e| PackError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        if member.is_directory {
            header.set_mode(0o755);
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
        } else {
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
        }
        header.set_path(&member.path)
            .map_err(|e| PackError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        header.set_cksum();
        tar_writer.append(&header, member.body.as_slice())?;
    }

    tar_writer.finish()?;
    Ok(())
}
```

### Algorithm

1. Sort members lexicographically by `path`.
2. Reject duplicate paths and absolute or empty paths.
3. Wrap `output` in a `GzBuilder` with `mtime = 0`, `operating_system = 0`, `Compression::new(6)`.
4. Wrap the gzip writer in a `tar::Builder` with `HeaderMode::Deterministic`.
5. For each member, build a `Header::new_ustar`, zero out timestamps and ownership, set the appropriate mode and entry type, write the body.
6. Finalise.

### Tests

- `export::tar_packer::tests::pack_is_byte_stable` — pack the same `Vec<TarMember>` twice; assert byte equality.
- `export::tar_packer::tests::lexicographic_order_enforced` — pack with members supplied in reverse order; extract; assert the order in the archive is lexicographic.
- `export::tar_packer::tests::duplicate_path_rejected` — pack two members with the same path; assert `PackError::DuplicatePath`.
- `export::tar_packer::tests::all_mtimes_zero` — pack; extract via `tar::Archive`; assert every entry's `mtime` is 0.
- `export::tar_packer::tests::all_uids_gids_zero` — same approach; assert uid/gid both 0.

### Acceptance command

```
cargo test -p trilithon-adapters export::tar_packer
```

### Exit conditions

- Pack is byte-stable on repeated calls.
- Lexicographic order is enforced.
- Duplicate paths and invalid paths are rejected with typed errors.
- All mtimes, uids, gids in the produced archive are zero.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- `bundle-format-v1.md` §2.

---

## Slice 25.6 [standard] — Master-key wrap (Argon2id + XChaCha20-Poly1305)

### Goal

`wrap_master_key(master_key, passphrase)` produces an opaque `Vec<u8>` envelope `[salt:32][nonce:24][ciphertext:N][tag:16]` per `bundle-format-v1.md` §8 and the phase reference. Argon2id parameters are `m=65536, t=3, p=4`. A symmetric `unwrap_master_key` validates the passphrase via Poly1305 authentication.

### Entry conditions

- Cargo dependencies `argon2 = "0.5"`, `chacha20poly1305 = "0.10"`, `rand = "0.8"` in `core/crates/adapters`.

### Files to create or modify

- `core/crates/adapters/src/export/master_key_wrap.rs` — wrap and unwrap.

### Signatures and shapes

```rust
//! Master-key wrap and unwrap.
//!
//! On-disk layout per bundle-format-v1.md §8:
//!
//! ```text
//! [ salt       : 32 bytes ]
//! [ nonce      : 24 bytes ]
//! [ ciphertext :  N bytes ]
//! [ tag        : 16 bytes ]
//! ```
//!
//! KDF: Argon2id with `m=65536, t=3, p=4`. AEAD: XChaCha20-Poly1305.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::{CryptoRng, Rng, RngCore};

#[derive(Debug, thiserror::Error)]
pub enum WrapError {
    #[error("kdf failed: {0}")]
    Kdf(String),
    #[error("aead encryption failed")]
    Encrypt,
    #[error("authentication failed (wrong passphrase or corrupt envelope)")]
    Authentication,
    #[error("envelope too short: {0} bytes")]
    Truncated(usize),
}

const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const ARGON2_M_COST: u32 = 65536;
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 4;

/// Wrap `master_key` (32 bytes) under `passphrase`.
pub fn wrap_master_key<R: Rng + CryptoRng>(
    rng: &mut R,
    master_key: &[u8; 32],
    passphrase: &str,
) -> Result<Vec<u8>, WrapError> {
    let mut salt = [0u8; SALT_LEN];
    rng.fill_bytes(&mut salt);

    let derived = derive_key(passphrase, &salt)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&derived));

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    let ciphertext_with_tag = cipher
        .encrypt(nonce, Payload { msg: master_key, aad: b"trilithon-master-key-v1" })
        .map_err(|_| WrapError::Encrypt)?;
    // ciphertext_with_tag has the 16-byte tag appended.

    let mut out = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext_with_tag.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext_with_tag);
    Ok(out)
}

/// Unwrap an envelope produced by `wrap_master_key`. Returns the
/// 32-byte master key on success; `WrapError::Authentication` on
/// wrong passphrase or corrupted ciphertext.
pub fn unwrap_master_key(
    envelope: &[u8],
    passphrase: &str,
) -> Result<[u8; 32], WrapError> {
    if envelope.len() < SALT_LEN + NONCE_LEN + TAG_LEN {
        return Err(WrapError::Truncated(envelope.len()));
    }
    let (salt, rest) = envelope.split_at(SALT_LEN);
    let (nonce_bytes, ciphertext_with_tag) = rest.split_at(NONCE_LEN);

    let derived = derive_key(passphrase, salt)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&derived));
    let nonce = XNonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, Payload { msg: ciphertext_with_tag, aad: b"trilithon-master-key-v1" })
        .map_err(|_| WrapError::Authentication)?;
    if plaintext.len() != 32 {
        return Err(WrapError::Authentication);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&plaintext);
    Ok(out)
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], WrapError> {
    let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, Some(32))
        .map_err(|e| WrapError::Kdf(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; 32];
    argon.hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| WrapError::Kdf(e.to_string()))?;
    Ok(out)
}
```

### Algorithm

`wrap_master_key`:

1. Sample 32 random salt bytes.
2. Derive a 32-byte AEAD key via Argon2id `(m=65536, t=3, p=4)` with the salt.
3. Sample 24 random nonce bytes.
4. AEAD-encrypt the master key with associated data `"trilithon-master-key-v1"`.
5. Concatenate `salt || nonce || ciphertext_with_tag`.

`unwrap_master_key`:

1. Reject envelopes shorter than `32 + 24 + 16 = 72` bytes.
2. Split into salt, nonce, ciphertext+tag.
3. Re-derive the AEAD key.
4. Decrypt; on AEAD failure return `WrapError::Authentication`.

### Tests

- `export::master_key_wrap::tests::round_trip_recovers_key` — wrap and unwrap with the same passphrase; assert equality.
- `export::master_key_wrap::tests::wrong_passphrase_yields_authentication_error` — wrap with `"a"`, unwrap with `"b"`; assert `WrapError::Authentication`.
- `export::master_key_wrap::tests::corrupt_ciphertext_yields_authentication_error` — flip one bit in the envelope; assert `WrapError::Authentication`.
- `export::master_key_wrap::tests::truncated_envelope_typed_error` — pass 50-byte buffer; assert `WrapError::Truncated`.
- `export::master_key_wrap::tests::deterministic_with_seeded_rng` — using `rand_chacha::ChaCha20Rng::seed_from_u64`, wrap twice with the same seed and same key/passphrase; assert equal envelopes.

### Acceptance command

```
cargo test -p trilithon-adapters export::master_key_wrap
```

### Exit conditions

- Round-trip succeeds.
- Wrong passphrase, corrupt ciphertext, truncated envelope all produce typed errors.
- The function takes a `Rng` so tests can supply a deterministic seed.

### Audit kinds emitted

None at this layer.

### Tracing events emitted

None.

### Cross-references

- ADR-0014.
- `bundle-format-v1.md` §8.

---

## Slice 25.7 [cross-cutting] — Bundle exporter and named determinism test

### Goal

The full bundle pipeline assembles `manifest.json`, `desired-state.json`, `snapshots/<id>.json` files, `audit-log.ndjson`, optional `secrets-vault.encrypted`, optional `master-key-wrap.bin`, `README.txt`, and `bundle.SHA256SUMS` (last) into a deterministic `tar.gz` archive. The named determinism test `bundle::tests::deterministic_pack_is_byte_stable` against fixture `core/crates/adapters/tests/fixtures/bundle/sample.bundle.fixture.json` packs twice and asserts byte equality.

### Entry conditions

- Slices 25.1, 25.4, 25.5, 25.6 complete.

### Files to create or modify

- `core/crates/core/src/export/bundle.rs` — core-side pure assembly (manifest and `desired-state.json` rendering).
- `core/crates/adapters/src/export/bundle_packager.rs` — adapter-side I/O orchestration (gathers snapshots and audit rows from `Storage`, calls the wrap, calls the packer).
- `core/crates/adapters/src/export/bundle/mod.rs` — module declaration with `#[cfg(test)] mod tests;`.
- `core/crates/adapters/src/export/bundle/tests.rs` — the named determinism test.
- `core/crates/adapters/tests/fixtures/bundle/sample.bundle.fixture.json` — fixture.

### Signatures and shapes

```rust
// core/crates/core/src/export/bundle.rs
//! Pure-core assembly of the bundle's `manifest.json` and
//! `desired-state.json`. I/O happens in the adapter; this module is
//! deterministic and side-effect-free.

use crate::export::manifest::{BundleManifest, RedactionPosture, SecretsEncryption,
    KdfParams};
use crate::export::deterministic::{write_canonical_pretty, CanonicalJsonError};
use crate::DesiredState;

pub struct CoreBundleParts {
    pub manifest_bytes_with_placeholder: Vec<u8>,
    pub desired_state_bytes: Vec<u8>,
    pub manifest: BundleManifest,
}

pub struct BundleInputs<'a> {
    pub trilithon_version: &'a str,
    pub caddy_version: &'a str,
    pub exported_at_rfc3339: &'a str,
    pub root_snapshot_id: &'a str,
    pub source_installation_id: &'a str,
    pub snapshot_count: u32,
    pub audit_event_count: u32,
    pub secrets_included: bool,
    pub master_key_wrap_present: bool,
    pub caddy_admin_endpoint_at_export: &'a str,
    pub state: &'a DesiredState,
}

pub fn render_core_parts(
    inputs: &BundleInputs<'_>,
) -> Result<CoreBundleParts, CanonicalJsonError> {
    let manifest = BundleManifest {
        schema_version: 1,
        trilithon_version: inputs.trilithon_version.to_string(),
        caddy_version: inputs.caddy_version.to_string(),
        exported_at: inputs.exported_at_rfc3339.to_string(),
        root_snapshot_id: inputs.root_snapshot_id.to_string(),
        source_installation_id: inputs.source_installation_id.to_string(),
        snapshot_count: inputs.snapshot_count,
        audit_event_count: inputs.audit_event_count,
        secrets_included: inputs.secrets_included,
        secrets_encryption: if inputs.secrets_included {
            Some(SecretsEncryption {
                algorithm: "xchacha20poly1305".to_string(),
                kdf: "argon2id".to_string(),
                kdf_params: KdfParams { m_cost: 65536, t_cost: 3, p_cost: 4 },
            })
        } else {
            None
        },
        caddy_admin_endpoint_at_export: inputs.caddy_admin_endpoint_at_export.to_string(),
        bundle_sha256: BundleManifest::BUNDLE_SHA256_PLACEHOLDER.to_string(),
        master_key_wrap_present: inputs.master_key_wrap_present,
        redaction_posture: if inputs.secrets_included {
            RedactionPosture::SecretsIncludedEncrypted
        } else {
            RedactionPosture::SecretsExcluded
        },
    };

    let manifest_value = serde_json::to_value(&manifest)?;
    let mut manifest_bytes = Vec::new();
    write_canonical_pretty(&mut manifest_bytes, &manifest_value)?;

    let state_value = inputs.state.to_canonical_json_value()?;
    let mut state_bytes = Vec::new();
    write_canonical_pretty(&mut state_bytes, &state_value)?;

    Ok(CoreBundleParts {
        manifest_bytes_with_placeholder: manifest_bytes,
        desired_state_bytes: state_bytes,
        manifest,
    })
}
```

```rust
// core/crates/adapters/src/export/bundle_packager.rs
//! Adapter-side bundle packager.
//!
//! Authoritative spec: `docs/architecture/bundle-format-v1.md`.

use std::io::Write;

use trilithon_core::export::bundle::{render_core_parts, BundleInputs, CoreBundleParts};
use trilithon_core::storage::Storage;

use crate::export::tar_packer::{pack, PackError, TarMember};
use crate::export::master_key_wrap::wrap_master_key;

#[derive(Debug, thiserror::Error)]
pub enum BundleExportError {
    #[error("storage error: {0}")]
    Storage(#[from] trilithon_core::storage::StorageError),
    #[error("pack error: {0}")]
    Pack(#[from] PackError),
    #[error("wrap error: {0}")]
    Wrap(#[from] crate::export::master_key_wrap::WrapError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("render error: {0}")]
    Render(String),
}

pub struct BundleExportRequest<'a> {
    pub passphrase: Option<&'a str>,
    pub include_secrets: bool,
}

/// Pack a complete native bundle to `output`. The procedure is
/// substitute-and-re-pack (bundle-format-v1.md §4):
///
/// 1. Render every member except `bundle.SHA256SUMS` and pack with
///    the manifest carrying the 64-character ASCII-zero placeholder.
/// 2. Hash the resulting archive bytes.
/// 3. Substitute the real digest in place of the placeholder in
///    `manifest.json`. Append `bundle.SHA256SUMS` (computed against
///    every other member). Re-pack.
pub async fn export_bundle<W: Write>(
    storage: &dyn Storage,
    request: &BundleExportRequest<'_>,
    output: W,
) -> Result<(), BundleExportError> {
    // 1. Gather inputs from storage.
    // 2. Build CoreBundleParts via render_core_parts.
    // 3. Build the snapshot files, audit-log.ndjson, secrets blob,
    //    master-key-wrap.bin, README.txt.
    // 4. First pack (placeholder manifest).
    // 5. Hash; substitute; re-render manifest; recompute SHA256SUMS;
    //    second pack.
    todo!("implementation per bundle-format-v1.md")
}
```

```rust
// core/crates/adapters/src/export/bundle/tests.rs
//! Bundle determinism contract evidence.
//!
//! See bundle-format-v1.md §10. The negative self-test deliberately
//! re-introduces real `mtime` capture in the packer and asserts the
//! test fails.

use std::path::PathBuf;

#[test]
fn deterministic_pack_is_byte_stable() {
    let fixture_path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "bundle",
        "sample.bundle.fixture.json",
    ].iter().collect();
    let fixture = std::fs::read(&fixture_path)
        .expect("sample bundle fixture readable");

    let archive_a = pack_fixture_in_temp_clock(&fixture, /*now_unix=*/ 1_000_000_000);
    let archive_b = pack_fixture_in_temp_clock(&fixture, /*now_unix=*/ 2_000_000_000);
    assert_eq!(archive_a, archive_b,
        "two packs of the same fixture under two clocks MUST be byte-equal");
}

fn pack_fixture_in_temp_clock(fixture: &[u8], _now_unix: i64) -> Vec<u8> {
    // Implementation: deserialise fixture, drive export_bundle in a
    // temp directory whose system clock is set to `_now_unix`, capture
    // archive bytes. The packer ignores the clock; this test pins the
    // contract.
    todo!("implementation")
}
```

### Algorithm

1. Gather inputs from storage: every snapshot in the parent chain of the latest desired state, every audit row, the secrets blob (if `include_secrets`), the source installation id.
2. Render `manifest.json` with `bundle_sha256 = BUNDLE_SHA256_PLACEHOLDER`.
3. Render `desired-state.json` via the canonical writer.
4. For each snapshot, render its JSON file at path `snapshots/<id>.json`.
5. Render `audit-log.ndjson` sorted by `(created_at, id)` ascending, one JSON object per line.
6. If `include_secrets`, append the encrypted secrets blob at `secrets-vault.encrypted`.
7. If `passphrase` is supplied, wrap the master key via `wrap_master_key`, append at `master-key-wrap.bin`.
8. Render `README.txt` per `bundle-format-v1.md` §9.
9. First pack: assemble all members above (without `bundle.SHA256SUMS`), call `pack`, capture archive bytes.
10. Hash the archive bytes with SHA-256.
11. Substitute the real digest in place of the placeholder in `manifest.json`.
12. Compute `bundle.SHA256SUMS` listing SHA-256 of every other member.
13. Second pack: assemble all members plus the now-final manifest and the SHA256SUMS file (last), call `pack` into `output`.

### Tests

- `bundle::tests::deterministic_pack_is_byte_stable` — the named contract test.
- `bundle::tests::manifest_has_real_sha_after_repack` — the placeholder MUST NOT appear in the final archive.
- `bundle::tests::sha256sums_lists_every_other_member` — extract the final archive; parse `bundle.SHA256SUMS`; assert every member except `bundle.SHA256SUMS` itself is listed.
- `bundle::tests::passphrase_present_iff_master_key_wrap_present` — exporting without a passphrase produces a manifest with `master_key_wrap_present = false` and no `master-key-wrap.bin` member.

### Acceptance command

```
cargo test -p trilithon-adapters bundle::tests
```

### Exit conditions

- The named determinism test passes.
- The substitute-and-re-pack procedure produces a manifest whose `bundle_sha256` is the real digest.
- A negative self-test that re-introduces real `mtime` capture in the packer fails the determinism test.

### Audit kinds emitted

`export.bundle` (architecture §6.6) — wired in slice 25.8.

### Tracing events emitted

`http.request.received`, `http.request.completed` (architecture §12.1) — at the HTTP boundary in slice 25.9.

### Cross-references

- `bundle-format-v1.md` (entire document — authoritative).
- ADR-0009, ADR-0014.

---

## Slice 25.8 [cross-cutting] — Audit kinds plus artefact SHA-256 persistence

### Goal

`AuditEvent` gains three variants — `ExportCaddyJson`, `ExportCaddyfile`, `ExportBundle` — each persisting `notes` JSON `{ format, byte_size, sha256_of_artifact, redaction_posture, snapshot_id_at_export, warning_count }`. Every export handler computes the SHA-256 of the response bytes and writes it to the audit row.

### Entry conditions

- Slices 25.2, 25.3, 25.7 complete.
- The `AuditEvent` enum exists from Phase 6.

### Files to create or modify

- `core/crates/core/src/audit.rs` — add the three variants with their `Display` impl returning the dotted kind strings exactly.

### Signatures and shapes

```rust
// core/crates/core/src/audit.rs
#[derive(Debug, Clone)]
pub enum AuditEvent {
    // ... existing variants ...
    ExportCaddyJson(ExportNotes),
    ExportCaddyfile(ExportNotes),
    ExportBundle(ExportNotes),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportNotes {
    pub format: ExportFormat,
    pub byte_size: u64,
    pub sha256_of_artifact: String,        // lowercase hex
    pub redaction_posture: RedactionPosture,
    pub snapshot_id_at_export: String,
    pub warning_count: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportFormat {
    CaddyJson,
    Caddyfile,
    Bundle,
}

impl std::fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // ...
            AuditEvent::ExportCaddyJson(_) => f.write_str("export.caddy-json"),
            AuditEvent::ExportCaddyfile(_) => f.write_str("export.caddyfile"),
            AuditEvent::ExportBundle(_)    => f.write_str("export.bundle"),
        }
    }
}
```

### Algorithm

1. After producing the export bytes, compute `sha2::Sha256::digest(&bytes)`, hex-encode lowercase.
2. Build an `ExportNotes` with the format, byte size, hash, redaction posture, current snapshot id, count of `LossyWarning`s.
3. Append an audit row via `record_audit_event` with the matching variant.

### Tests

- `audit::tests::export_caddy_json_kind_string_exact` — `AuditEvent::ExportCaddyJson(...).to_string() == "export.caddy-json"`.
- `audit::tests::export_caddyfile_kind_string_exact` — `"export.caddyfile"`.
- `audit::tests::export_bundle_kind_string_exact` — `"export.bundle"`.
- Integration test in slice 25.9 asserts each handler writes exactly one row whose `sha256_of_artifact` matches an independent SHA-256 of the response bytes.

### Acceptance command

```
cargo test -p trilithon-core audit::tests::export
```

### Exit conditions

- The three variants exist and `Display` produces the §6.6 dotted strings.
- Notes shape matches the phase-reference contract.

### Audit kinds emitted

- `export.caddy-json`, `export.caddyfile`, `export.bundle` (architecture §6.6).

### Tracing events emitted

None at this layer.

### Cross-references

- Architecture §6.6.
- ADR-0009.

---

## Slice 25.9 [cross-cutting] — HTTP export endpoints (three formats plus warnings sidecar)

### Goal

Five HTTP endpoints expose the export pipeline:

- `GET /api/v1/export/caddy-json` — Caddy JSON; 16 MiB cap.
- `GET /api/v1/export/caddyfile` — Caddyfile body with `X-Trilithon-Lossy-Warnings` header; 8 MiB cap.
- `GET /api/v1/export/caddyfile/warnings` — sidecar text.
- `GET /api/v1/export/bundle` — passphrase-less bundle (no `master-key-wrap.bin`).
- `POST /api/v1/export/bundle` — passphrase-protected bundle; 256 MiB cap unless `allow_large_bundle = true`.

### Entry conditions

- Slices 25.2, 25.3, 25.7, 25.8 complete.
- The HTTP server (Phase 9) is in place.

### Files to create or modify

- `core/crates/cli/src/http/export.rs` — handlers.
- `core/crates/cli/src/http/router.rs` — register the routes.

### Signatures and shapes

```rust
//! HTTP export handlers.
//!
//! Per phase 25 reference: every export call writes one audit row
//! whose `sha256_of_artifact` matches the response bytes.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use sha2::{Digest, Sha256};

const MAX_CADDY_JSON_BYTES: usize = 16 * 1024 * 1024;
const MAX_CADDYFILE_BYTES: usize  =  8 * 1024 * 1024;
const MAX_BUNDLE_BYTES: usize     = 256 * 1024 * 1024;

pub async fn get_caddy_json(
    State(app): State<AppState>,
) -> Result<Response, ExportHttpError> {
    let state = app.storage().latest_desired_state().await?
        .ok_or(ExportHttpError::NoDesiredState)?;
    let bytes = trilithon_core::export::caddy_json::export_caddy_json(&state.desired_state)?;
    if bytes.len() > MAX_CADDY_JSON_BYTES {
        return Err(ExportHttpError::TooLarge { actual: bytes.len(), cap: MAX_CADDY_JSON_BYTES });
    }
    let hash = hex::encode(Sha256::digest(&bytes));
    app.audit().record_export_caddy_json(&bytes, &hash, &state).await?;
    let filename = format!(
        "caddy-config-{}-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        &state.id[..12],
    );
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|_| ExportHttpError::HeaderEncoding)?,
    );
    Ok((StatusCode::OK, headers, Body::from(bytes)).into_response())
}

pub async fn get_caddyfile(
    State(app): State<AppState>,
) -> Result<Response, ExportHttpError> {
    let state = app.storage().latest_desired_state().await?
        .ok_or(ExportHttpError::NoDesiredState)?;
    let result = trilithon_core::caddyfile::printer::print(&state.desired_state);
    let bytes = result.caddyfile.into_bytes();
    if bytes.len() > MAX_CADDYFILE_BYTES {
        return Err(ExportHttpError::TooLarge { actual: bytes.len(), cap: MAX_CADDYFILE_BYTES });
    }
    let hash = hex::encode(Sha256::digest(&bytes));
    let warning_ids: Vec<String> = result.warnings.iter().map(|w| w.stable_id()).collect();
    app.audit().record_export_caddyfile(&bytes, &hash, &state, &warning_ids).await?;
    app.export_warnings_cache().store(&state.id, result.sidecar_warnings_text);

    let filename = format!(
        "caddyfile-{}-{}.caddyfile",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        &state.id[..12],
    );
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE,
        HeaderValue::from_static("text/caddyfile; charset=utf-8"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|_| ExportHttpError::HeaderEncoding)?,
    );
    headers.insert(
        "x-trilithon-lossy-warnings",
        HeaderValue::from_str(&warning_ids.join(","))
            .map_err(|_| ExportHttpError::HeaderEncoding)?,
    );
    Ok((StatusCode::OK, headers, Body::from(bytes)).into_response())
}

pub async fn get_caddyfile_warnings(
    State(app): State<AppState>,
) -> Result<Response, ExportHttpError> {
    let snapshot_id = app.storage().latest_desired_state().await?
        .ok_or(ExportHttpError::NoDesiredState)?.id;
    let body = app.export_warnings_cache().get(&snapshot_id)
        .unwrap_or_else(|| "No warnings cached for the current snapshot.\n".to_string());
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"));
    Ok((StatusCode::OK, headers, body).into_response())
}

#[derive(serde::Deserialize)]
pub struct PostBundleBody {
    pub passphrase: String,
    #[serde(default)]
    pub allow_large_bundle: bool,
}

pub async fn get_bundle(
    State(app): State<AppState>,
) -> Result<Response, ExportHttpError> {
    serve_bundle(app, /*passphrase=*/ None, /*allow_large=*/ false).await
}

pub async fn post_bundle(
    State(app): State<AppState>,
    Json(body): Json<PostBundleBody>,
) -> Result<Response, ExportHttpError> {
    serve_bundle(app, Some(&body.passphrase), body.allow_large_bundle).await
}

async fn serve_bundle(
    app: AppState,
    passphrase: Option<&str>,
    allow_large: bool,
) -> Result<Response, ExportHttpError> {
    let mut buffer = Vec::new();
    trilithon_adapters::export::bundle_packager::export_bundle(
        app.storage(),
        &trilithon_adapters::export::bundle_packager::BundleExportRequest {
            passphrase,
            include_secrets: passphrase.is_some(),
        },
        &mut buffer,
    ).await?;
    if !allow_large && buffer.len() > MAX_BUNDLE_BYTES {
        return Err(ExportHttpError::TooLarge { actual: buffer.len(), cap: MAX_BUNDLE_BYTES });
    }
    let hash = hex::encode(Sha256::digest(&buffer));
    app.audit().record_export_bundle(&buffer, &hash, passphrase.is_some()).await?;
    let filename = format!(
        "trilithon-bundle-{}-{}.tar.gz",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        &hash[..12],
    );
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/gzip"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|_| ExportHttpError::HeaderEncoding)?,
    );
    Ok((StatusCode::OK, headers, Body::from(buffer)).into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum ExportHttpError {
    #[error("no desired state to export")]
    NoDesiredState,
    #[error("storage: {0}")]
    Storage(#[from] trilithon_core::storage::StorageError),
    #[error("export: {0}")]
    Export(#[from] trilithon_core::export::caddy_json::ExportError),
    #[error("bundle: {0}")]
    Bundle(#[from] trilithon_adapters::export::bundle_packager::BundleExportError),
    #[error("payload too large: {actual} bytes > cap {cap}")]
    TooLarge { actual: usize, cap: usize },
    #[error("header encoding")]
    HeaderEncoding,
}

impl IntoResponse for ExportHttpError {
    fn into_response(self) -> Response {
        let status = match &self {
            ExportHttpError::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            ExportHttpError::NoDesiredState  => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(serde_json::json!({ "error": self.to_string() }))).into_response()
    }
}
```

### Algorithm

For each handler:

1. Fetch the latest `DesiredState` from `Storage`.
2. Call the corresponding pure exporter (`export_caddy_json`, `printer::print`, `bundle_packager::export_bundle`).
3. Enforce the format-specific size cap; return `413` over the cap.
4. Compute SHA-256 of the response bytes; record the audit row with that hash.
5. Build the response with the documented `Content-Type` and `Content-Disposition` (and `X-Trilithon-Lossy-Warnings` for Caddyfile).

### Tests

- `core/crates/cli/tests/http_export_caddy_json.rs` — issue `GET`; assert 200, content-type, content-disposition, audit row written with matching hash.
- `core/crates/cli/tests/http_export_caddy_json_too_large.rs` — synthesise an over-cap state; assert 413.
- `core/crates/cli/tests/http_export_caddyfile.rs` — assert response, header presence, sidecar endpoint returns the warnings text.
- `core/crates/cli/tests/http_export_bundle_get.rs` — `GET` returns a passphrase-less bundle; manifest's `master_key_wrap_present = false`.
- `core/crates/cli/tests/http_export_bundle_post.rs` — `POST` with passphrase; manifest's `master_key_wrap_present = true`.
- `core/crates/cli/tests/http_export_bundle_too_large.rs` — without `allow_large_bundle`; assert 413.

### Acceptance command

```
cargo test -p trilithon-cli http_export
```

### Exit conditions

- Five endpoints exist and pass their integration tests.
- Every handler writes exactly one audit row whose `sha256_of_artifact` matches the response bytes.
- Size caps are enforced.

### Audit kinds emitted

- `export.caddy-json`, `export.caddyfile`, `export.bundle` (architecture §6.6).

### Tracing events emitted

- `http.request.received`, `http.request.completed` (architecture §12.1) — at the HTTP server boundary.

### Cross-references

- ADR-0009.
- PRD T2.9.
- Architecture §6.6, §12.1.
- Hazards: H7, H10.

---

## Slice 25.10 [cross-cutting] — CLI `trilithon export` subcommand

### Goal

`trilithon export --format {caddy-json|caddyfile|bundle} --out <path> [--passphrase-stdin] [--allow-large-bundle]` runs the export pipeline either by calling the daemon over loopback (when reachable) or in-process against the local SQLite file.

### Entry conditions

- Slice 25.9 complete.

### Files to create or modify

- `core/crates/cli/src/commands/export.rs` — subcommand.

### Signatures and shapes

```rust
//! `trilithon export` subcommand.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum ExportCliFormat {
    CaddyJson,
    Caddyfile,
    Bundle,
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    #[arg(long, value_enum)]
    pub format: ExportCliFormat,
    #[arg(long)]
    pub out: PathBuf,
    /// Read a passphrase from stdin (bundle format only). Mutually
    /// exclusive with no-passphrase bundles.
    #[arg(long)]
    pub passphrase_stdin: bool,
    /// Permit bundles over the 256 MiB cap.
    #[arg(long)]
    pub allow_large_bundle: bool,
}

pub async fn run(args: ExportArgs) -> ExitCode {
    let passphrase = if args.passphrase_stdin {
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            eprintln!("trilithon export: failed to read passphrase from stdin");
            return ExitCode::FAILURE;
        }
        Some(buf.trim_end_matches('\n').to_string())
    } else {
        None
    };

    let bytes = match dispatch(&args, passphrase.as_deref()).await {
        Ok(b)  => b,
        Err(e) => { eprintln!("trilithon export: {e}"); return ExitCode::FAILURE; }
    };
    if let Err(e) = std::fs::write(&args.out, &bytes) {
        eprintln!("trilithon export: writing {}: {e}", args.out.display());
        return ExitCode::FAILURE;
    }
    println!("trilithon export: wrote {} bytes to {}", bytes.len(), args.out.display());
    ExitCode::SUCCESS
}

async fn dispatch(
    args: &ExportArgs,
    passphrase: Option<&str>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // 1. Try loopback daemon at 127.0.0.1:7878.
    // 2. On connection refused, run the export pipeline in-process
    //    against the local SQLite database.
    todo!("implementation per phase reference")
}
```

### Algorithm

1. Parse the subcommand args.
2. If `--passphrase-stdin`, read stdin to EOF, trim trailing newline.
3. Try the loopback daemon; on connection refused, fall back to in-process pipeline against the SQLite database.
4. Write the response bytes to `--out`.

### Tests

- `core/crates/cli/tests/cli_export_caddy_json.rs` — invokes the binary in-process against a fixture database; asserts the output file exists and matches a golden file.
- `core/crates/cli/tests/cli_export_bundle_with_passphrase.rs` — pipes a passphrase via stdin; asserts the output file is a valid `.tar.gz`.

### Acceptance command

```
cargo test -p trilithon-cli cli_export
```

### Exit conditions

- The three formats are routable from the CLI.
- Output files are written to `--out`.
- Daemon-less invocation produces the same byte output as the HTTP path.

### Audit kinds emitted

When the daemon path is taken: same as slice 25.9. When the in-process path is taken, the CLI MUST write the same audit row directly via the `Storage` trait so the audit record exists regardless.

### Tracing events emitted

`daemon.started` is NOT emitted by the CLI (it does not run a daemon). HTTP-path tracing events are emitted by the daemon if used.

### Cross-references

- PRD T2.9.

---

## Slice 25.11 [standard] — Web UI `ExportPanel`

### Goal

A React component at `web/src/features/export/ExportPanel.tsx` exposes three buttons (Caddy JSON, Caddyfile, Native bundle), reveals a passphrase entry and an "include redacted secrets" toggle when the bundle button is selected, and shows a downloads list with the audit-row hash for each past export.

### Entry conditions

- Slice 25.9 complete.
- The web shell (Phase 11) exists with auth and the `useApi` hook.

### Files to create or modify

- `web/src/features/export/ExportPanel.tsx` — component.
- `web/src/features/export/ExportPanel.test.tsx` — Vitest test.
- `web/src/features/export/index.ts` — barrel.

### Signatures and shapes

```tsx
import { useState } from 'react';

interface ExportRow {
  readonly snapshotId: string;
  readonly format: 'caddy-json' | 'caddyfile' | 'bundle';
  readonly sha256: string;
  readonly byteSize: number;
  readonly exportedAt: string;
}

interface ExportPanelProps {
  readonly history: readonly ExportRow[];
  readonly onExportCaddyJson: () => Promise<void>;
  readonly onExportCaddyfile: () => Promise<void>;
  readonly onExportBundle: (passphrase: string | null) => Promise<void>;
}

export function ExportPanel(props: ExportPanelProps): JSX.Element {
  const [bundleOpen, setBundleOpen] = useState<boolean>(false);
  const [passphrase, setPassphrase] = useState<string>('');
  const [includeSecrets, setIncludeSecrets] = useState<boolean>(false);

  return (
    <section aria-labelledby="export-panel-title" className="space-y-4">
      <h2 id="export-panel-title" className="text-lg font-semibold">
        Export configuration
      </h2>

      <div className="flex gap-2">
        <button type="button" onClick={() => { void props.onExportCaddyJson(); }}>
          Export Caddy JSON
        </button>
        <button type="button" onClick={() => { void props.onExportCaddyfile(); }}>
          Export Caddyfile
        </button>
        <button type="button" onClick={() => setBundleOpen((open) => !open)}>
          Export native bundle
        </button>
      </div>

      {bundleOpen ? (
        <fieldset className="border p-2">
          <legend>Native bundle options</legend>
          <label className="block">
            Passphrase
            <input
              type="password"
              value={passphrase}
              onChange={(event) => setPassphrase(event.target.value)}
            />
          </label>
          <label className="block">
            <input
              type="checkbox"
              checked={includeSecrets}
              onChange={(event) => setIncludeSecrets(event.target.checked)}
            />
            Include redacted secrets (encrypted)
          </label>
          <button
            type="button"
            onClick={() => {
              void props.onExportBundle(includeSecrets ? passphrase : null);
            }}
          >
            Create bundle
          </button>
        </fieldset>
      ) : null}

      <h3 className="text-md font-semibold">Past exports</h3>
      <ul>
        {props.history.map((row) => (
          <li key={`${row.snapshotId}-${row.format}-${row.exportedAt}`}>
            <code>{row.format}</code>
            {' '}
            <span>{row.exportedAt}</span>
            {' '}
            <span title="SHA-256">{row.sha256.slice(0, 12)}…</span>
            {' '}
            <span>{row.byteSize} bytes</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
```

### Algorithm

1. Render three buttons.
2. Toggle a fieldset on the bundle button.
3. The fieldset hosts a passphrase input and an "include redacted secrets" checkbox.
4. Render a list of past exports with format, timestamp, truncated hash, byte size.

### Tests

- `ExportPanel.test.tsx`:
  - `renders three export buttons`.
  - `clicking Caddy JSON calls onExportCaddyJson`.
  - `clicking Caddyfile calls onExportCaddyfile`.
  - `clicking Native bundle reveals passphrase entry and toggle`.
  - `submitting bundle with no secrets calls onExportBundle(null)`.
  - `submitting bundle with secrets and passphrase calls onExportBundle("<passphrase>")`.
  - `history rows render the format, hash prefix, and byte size`.

### Acceptance command

```
pnpm vitest run web/src/features/export/ExportPanel.test.tsx
```

### Exit conditions

- The component renders the three buttons.
- The bundle reveal flow works.
- All seven Vitest cases pass.

### Audit kinds emitted

The component itself does not emit audit rows; it triggers HTTP calls handled by slice 25.9.

### Tracing events emitted

None at the React layer.

### Cross-references

- PRD T2.9.

---

## Slice 25.12 [standard] — Caddyfile round-trip integration test against Phase 13 corpus

### Goal

For every non-pathological fixture in the Phase 13 corpus, the Caddyfile export → Phase 13 parser → Caddy JSON export MUST match the original Caddy JSON export modulo the documented non-equivalences in `docs/architecture/caddyfile-translation.md`.

### Entry conditions

- Slices 25.2 and 25.3 complete.
- The Phase 13 fixture corpus exists under `core/crates/adapters/tests/fixtures/caddyfile/`.

### Files to create or modify

- `core/crates/adapters/tests/export_caddyfile_round_trip.rs` — the integration test.

### Signatures and shapes

```rust
//! Caddyfile round-trip integration test.
//!
//! For every non-pathological fixture in the Phase 13 corpus:
//!
//! 1. Render the source `DesiredState` to Caddyfile via the printer.
//! 2. Parse the Caddyfile back through the Phase 13 parser.
//! 3. Render the parsed result to Caddy JSON via slice 25.2.
//! 4. Render the original `DesiredState` to Caddy JSON.
//! 5. Compute the diff under the documented non-equivalence policy.
//! 6. Assert the diff is empty.

use std::path::PathBuf;

#[test]
fn caddyfile_round_trip_preserves_semantics() {
    let corpus_root: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests", "fixtures", "caddyfile",
    ].iter().collect();
    let mut failures = Vec::new();
    for fixture in iter_non_pathological_fixtures(&corpus_root) {
        if let Err(diff) = round_trip_one(&fixture) {
            failures.push((fixture, diff));
        }
    }
    assert!(failures.is_empty(),
        "Caddyfile round-trip failed for {} fixtures: {:?}",
        failures.len(), failures);
}

fn round_trip_one(fixture: &PathBuf) -> Result<(), String> {
    // Implementation per algorithm.
    todo!()
}

fn iter_non_pathological_fixtures(_root: &PathBuf) -> Vec<PathBuf> {
    todo!()
}
```

### Algorithm

For each fixture file:

1. Load `DesiredState`.
2. Print to Caddyfile via slice 25.3.
3. Parse the Caddyfile via Phase 13's parser.
4. Render the parsed `DesiredState` to Caddy JSON via slice 25.2.
5. Render the original `DesiredState` to Caddy JSON.
6. Diff the two byte-streams; subtract any path listed as `lossy` or `unsupported` in `docs/architecture/caddyfile-translation.md`.
7. Pass iff the residual diff is empty.

### Tests

- The single integration test enumerates the corpus.
- A negative test (separate `#[test]`) deliberately introduces a `route.named-matcher-composition` divergence that is documented as `lossy`; assert the round-trip still passes (the divergence is excused).

### Acceptance command

```
cargo test -p trilithon-adapters --test export_caddyfile_round_trip
```

### Exit conditions

- The round-trip passes for every non-pathological fixture.
- The documented non-equivalences are subtracted from the diff.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T2.9.
- Hazards: H7.

---

## Slice 25.13 [trivial] — Migration documentation page

### Goal

`docs/migrating-off-trilithon.md` is the walk-away page for users leaving Trilithon. After this slice, a documentation lint asserts every required heading is present.

### Entry conditions

- Slices 25.9 and 25.7 complete (so the page can reference real operations).

### Files to create or modify

- `docs/migrating-off-trilithon.md` — the page.
- `docs/test/lint-migrating-off-trilithon-headings.sh` — heading lint.

### Signatures and shapes

`docs/migrating-off-trilithon.md` outline:

```markdown
# Migrating off Trilithon

This page documents how to leave Trilithon at any time with a working
Caddy configuration in hand.

## Choosing an export format

## Pointing stock Caddy at the JSON export

## Editing the Caddyfile export by hand

## Re-importing a bundle into another Trilithon

## What you keep, what you lose

## Verifying the export against the audit log
```

`docs/test/lint-migrating-off-trilithon-headings.sh` (verbatim):

```bash
#!/usr/bin/env bash
set -euo pipefail
required=(
    "## Choosing an export format"
    "## Pointing stock Caddy at the JSON export"
    "## Editing the Caddyfile export by hand"
    "## Re-importing a bundle into another Trilithon"
    "## What you keep, what you lose"
    "## Verifying the export against the audit log"
)
for heading in "${required[@]}"; do
    if ! grep -Fxq "${heading}" docs/migrating-off-trilithon.md; then
        echo "lint: missing heading: ${heading}" >&2
        exit 1
    fi
done
```

### Algorithm

1. Author the page with every required heading.
2. The lint enforces presence on every CI run.

### Tests

- `docs/test/lint-migrating-off-trilithon-headings.sh` — passes on canonical content; fails on a synthetic deletion.

### Acceptance command

```
bash docs/test/lint-migrating-off-trilithon-headings.sh
```

### Exit conditions

- The page exists with every required heading.
- The lint passes.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T2.9 (mitigates H7).
- Hazards: H7.

---

## Phase exit checklist

- [ ] `just check` passes.
- [ ] Caddy JSON export, applied to a fresh Caddy, produces identical runtime behaviour against the Phase 13 request matrix (slice 25.2).
- [ ] Caddyfile export round-trips for every supported-subset fixture; lossy warnings are surfaced (slice 25.12).
- [ ] Native bundle export is byte-deterministic across two consecutive invocations: `bundle::tests::deterministic_pack_is_byte_stable` passes (slice 25.7).
- [ ] The bundle's `master-key-wrap.bin` is present iff a passphrase was supplied; verified by `master_key_wrap_present` in the manifest (slice 25.7, slice 25.9).
- [ ] Every export emits exactly one audit row whose `sha256_of_artifact` matches the downloaded bytes (slice 25.8, slice 25.9).
- [ ] `docs/migrating-off-trilithon.md` exists with every required heading (slice 25.13).
- [ ] Every `LossyWarning::CaddyfileExportLoss { construct }` value has a row in `docs/architecture/caddyfile-translation.md` (slice 25.3).
- [ ] The HTTP endpoints enforce the documented size caps and respond `413 Payload Too Large` over the cap (slice 25.9).

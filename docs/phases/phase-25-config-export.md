# Phase 25 — Configuration export (JSON, Caddyfile, native bundle)

Source of truth: [`../phases/phased-plan.md#phase-25--configuration-export-json-caddyfile-native-bundle`](../phases/phased-plan.md#phase-25--configuration-export-json-caddyfile-native-bundle).

## Pre-flight checklist

- [ ] Phase 24 complete.
- [ ] Phase 13 fixture corpus is in place; the Caddyfile printer reuses Phase 13's grammar definitions.
- [ ] Argon2 and XChaCha20-Poly1305 implementations (`argon2`, `chacha20poly1305`) are workspace dependencies of `core/crates/adapters/`.

## Tasks

### Caddy JSON serialiser

- [ ] **Implement the Caddy JSON exporter.**
  - Module: `core/crates/core/src/export/caddy_json.rs`.
  - Acceptance: `pub fn export_caddy_json(state: &DesiredState) -> Result<Vec<u8>, ExportError>`. Output MUST be deterministic: object keys sorted lexicographically, arrays in semantic order, `serde_json::to_vec_pretty` with two-space indent, no Trilithon-specific extensions (`@id` annotations stripped).
  - Done when: a unit test serialises a representative `DesiredState` twice and asserts byte-identical output, and a separate test asserts the result loads cleanly into Caddy 2.8 via `caddy run --config`.
  - Feature: T2.9.
- [ ] **Implement deterministic-ordering helper.**
  - Module: `core/crates/core/src/export/deterministic.rs`.
  - Acceptance: `pub fn canonical_json_writer<W: Write>(w: W) -> CanonicalJsonWriter<W>` returns a writer that sorts object keys recursively and uses canonical number formatting. Reused by the Caddy JSON exporter and the bundle's `desired-state.json`.
  - Done when: a unit test round-trips multiple JSON shapes and asserts ordering.
  - Feature: T2.9.

### Caddyfile printer

- [ ] **Implement the Caddyfile printer.**
  - Module: `core/crates/core/src/caddyfile/printer.rs`.
  - Acceptance: `pub fn print(state: &DesiredState) -> PrintResult`. `PrintResult { caddyfile: String, warnings: Vec<LossyWarning>, sidecar_warnings_text: String }`. The output MUST start with the leading comment block specified in the phased-plan section.
  - Done when: a unit test against a representative `DesiredState` asserts the leading comment and the produced Caddyfile parses back through Phase 13's parser.
  - Feature: T2.9 (mitigates H7).
- [ ] **Implement snippet deduplication helper.**
  - Module: `core/crates/core/src/caddyfile/printer/snippets.rs`.
  - Acceptance: `pub fn extract_snippets(routes: &[Route]) -> SnippetSet`. Header sets that appear on more than one route are extracted as snippets. Threshold: two appearances.
  - Done when: a unit test asserts two appearances triggers extraction; one appearance does not.
  - Feature: T2.9.
- [ ] **Author the Caddyfile-translation reference document.**
  - Path: `docs/architecture/caddyfile-translation.md`.
  - Acceptance: A field-by-field table of every Trilithon construct and its Caddyfile mapping, marking each row clean / lossy / unsupported.
  - Done when: a documentation lint asserts every `LossyWarning::CaddyfileExportLoss { construct }` value has a row.
  - Feature: T2.9.

### Native bundle

- [ ] **Author the bundle format specification page.**
  - Path: `docs/architecture/bundle-format-v1.md`.
  - Acceptance: The page is the field-by-field source of truth for the v1 bundle format. It MUST declare `schema_version: 1` STABLE for V1.0, state the V1.x and V2.x compatibility promise, document tar/gzip determinism rules, document the top-level archive layout, and specify each member's schema. The implementation MUST NOT diverge from this page; any deviation is an implementation bug.
  - Done when: the file exists and the documentation lint asserts every required heading.
  - Feature: T2.9.
- [ ] **Bundle determinism test (named).**
  - Module: `core/crates/adapters/src/export/bundle/tests.rs`.
  - Acceptance: Test name MUST be `bundle::tests::deterministic_pack_is_byte_stable`. Fixture path: `core/crates/adapters/tests/fixtures/bundle/sample.bundle.fixture.json`. The test packs the fixture twice in two different temporary directories with two different system clocks (the packer ignores both) and asserts the two archives are byte-identical.
  - Done when: the test passes; a deliberately-non-deterministic packer mutation (for example, preserving real `mtime`) MUST cause the test to fail in a fixture-driven self-test.
  - Feature: T2.9.

- [ ] **Author the manifest schema.**
  - Module: `core/crates/core/src/export/manifest.rs` and `docs/schemas/bundle-manifest.json`.
  - Acceptance: Rust type `BundleManifest { schema_version: u32, trilithon_version: String, caddy_version: String, source_installation_id: String, root_snapshot_id: String, exported_at_unix_seconds: i64, snapshot_count: u32, audit_row_count: u32, redaction_posture: RedactionPosture, master_key_wrap_present: bool }`. JSON Schema document accompanies it.
  - Done when: a unit test serialises and validates the manifest against the JSON Schema.
  - Feature: T2.9.
- [ ] **Implement the archive packer.**
  - Module: `core/crates/adapters/src/export/tar_packer.rs`.
  - Acceptance: `pub fn pack(members: Vec<TarMember>, output: impl Write) -> Result<(), PackError>`. Members sorted lexicographically. Every entry: `mtime = 0`, `uid = 0`, `gid = 0`, file mode `0o644`, directory mode `0o755`. No PAX extended headers, no global headers. Gzip layer: no filename, mtime zero, OS byte zero, level 9.
  - Done when: a unit test packs the same input twice and asserts byte-identical output.
  - Feature: T2.9.
- [ ] **Implement the bundle exporter.**
  - Module: `core/crates/core/src/export/bundle.rs` and `core/crates/adapters/src/export/bundle_packager.rs`.
  - Acceptance: Adapter side gathers the snapshots, audit log, encrypted secrets blob, and (if a passphrase is supplied) produces the master-key wrap. Core side produces `manifest.json` and `desired-state.json`. The packager assembles members in the order specified by the phased plan, writes `bundle.SHA256SUMS` last.
  - Done when: an integration test extracts the archive and asserts every member.
  - Feature: T2.9.
- [ ] **Implement the master-key wrap.**
  - Module: `core/crates/adapters/src/export/master_key_wrap.rs`.
  - Acceptance: `pub fn wrap_master_key(master_key: &[u8; 32], passphrase: &str) -> Result<Vec<u8>, WrapError>`. Argon2id `m=65536, t=3, p=4`, salt 32 random bytes from a CSPRNG. XChaCha20-Poly1305 nonce 24 random bytes. On-disk layout `[salt:32][nonce:24][ciphertext:N][tag:16]`. Pure-ish: takes a `Rng` for testability.
  - Done when: a unit test wraps and unwraps round-trip; a wrong-passphrase test asserts `WrapError::Authentication`.
  - Feature: T2.9.

### CLI

- [ ] **Wire the `trilithon export` subcommand.**
  - Module: `core/crates/cli/src/commands/export.rs`.
  - Acceptance: `trilithon export --format {caddy-json|caddyfile|bundle} --out <path> [--passphrase-stdin] [--allow-large-bundle]`. The CLI invokes the same code paths as the HTTP handlers; on a daemon-less invocation, runs the export pipeline in-process against the local SQLite file.
  - Done when: an integration test exercises each format and asserts the output file.
  - Feature: T2.9.

### HTTP API

- [ ] **Implement `GET /api/v1/export/caddy-json`.**
  - Module: `core/crates/cli/src/http/export.rs`.
  - Acceptance: Returns the bytes produced by `export_caddy_json`. `Content-Type: application/json`. `Content-Disposition: attachment; filename="caddy-config-<timestamp>-<short-snapshot-hash>.json"`. Enforces the 16 MiB hard cap; returns `413` over the cap.
  - Done when: an integration test asserts the body, headers, and over-cap rejection.
  - Feature: T2.9.
- [ ] **Implement `GET /api/v1/export/caddyfile` and the warnings sidecar.**
  - Module: `core/crates/cli/src/http/export.rs`.
  - Acceptance: Returns the printer's `caddyfile` body with `Content-Type: text/caddyfile; charset=utf-8`. The response includes `X-Trilithon-Lossy-Warnings: <comma-separated stable identifiers>`. A separate endpoint `GET /api/v1/export/caddyfile/warnings` returns the sidecar text. Enforces the 8 MiB cap.
  - Done when: an integration test asserts both endpoints and the header.
  - Feature: T2.9 (mitigates H7).
- [ ] **Implement `GET /api/v1/export/bundle` (passphrase-less) and `POST /api/v1/export/bundle` (with passphrase).**
  - Module: `core/crates/cli/src/http/export.rs`.
  - Acceptance: `GET` returns a passphrase-less bundle without `master-key-wrap.bin`; the manifest's `master_key_wrap_present = false`. `POST` accepts `{ passphrase: String, allow_large_bundle: bool }`, returns the bundle with `master-key-wrap.bin` and `master_key_wrap_present = true`. Enforces the 256 MiB cap unless `allow_large_bundle = true`.
  - Done when: integration tests cover both paths and over-cap rejection.
  - Feature: T2.9.

### Audit

- [ ] **Add export audit kinds.**
  - Module: `core/crates/core/src/audit.rs`.
  - Acceptance: `AuditKind::ExportCaddyJson`, `ExportCaddyfile`, `ExportBundle`. Each persists `notes = JSON{ format, byte_size, sha256_of_artifact, redaction_posture, snapshot_id_at_export, warning_count }`.
  - Done when: unit tests assert the kinds and notes shape.
  - Feature: T2.9 / T1.7.
- [ ] **Compute and persist artifact SHA-256.**
  - Module: `core/crates/cli/src/http/export.rs`.
  - Acceptance: Every export handler MUST compute the SHA-256 of the response bytes and write it to the audit row. The user can later verify the artifact against the audit row.
  - Done when: an integration test downloads an artifact, computes its SHA-256, and matches the audit row.
  - Feature: T2.9 / T1.7.

### Web UI

- [ ] **Implement `ExportPanel`.**
  - Path: `web/src/features/export/ExportPanel.tsx`.
  - Acceptance: `export function ExportPanel(): JSX.Element`. Three buttons (Caddy JSON, Caddyfile, Native bundle). The bundle button reveals a passphrase entry and an "include redacted secrets" toggle. A downloads list shows past exports with their audit-row hash.
  - Done when: a Vitest component test exercises the three buttons and the passphrase reveal.
  - Feature: T2.9.

### Round-trip and determinism tests

- [ ] **Caddy JSON round-trip against the Phase 13 corpus.**
  - Module: `core/crates/adapters/tests/export_caddy_json_round_trip.rs`.
  - Acceptance: For every non-pathological fixture, the Caddy JSON export loaded into a fresh `caddy run` MUST produce identical request-matrix responses to the source instance.
  - Done when: the test passes for every fixture.
  - Feature: T2.9.
- [ ] **Caddyfile round-trip against the Phase 13 corpus.**
  - Module: `core/crates/adapters/tests/export_caddyfile_round_trip.rs`.
  - Acceptance: For every non-pathological fixture in the supported subset, the Caddyfile export → Phase 13 parser → Caddy JSON export MUST match the original Caddy JSON export modulo the documented non-equivalences.
  - Done when: the test passes; documented non-equivalences are absent from the diff or marked accepted.
  - Feature: T2.9 (mitigates H7).
- [ ] **Bundle determinism test.**
  - Module: `core/crates/adapters/tests/export_bundle_determinism.rs`.
  - Acceptance: Exporting the same `DesiredState` twice in a row MUST produce byte-identical archives. The test extracts both, hashes each member, asserts equality, and hashes the archive file as a whole.
  - Done when: the test passes.
  - Feature: T2.9.
- [ ] **Bundle round-trip test (export → wipe → import).**
  - Module: `core/crates/adapters/tests/export_bundle_round_trip.rs`.
  - Acceptance: After Phase 26's restore is wired, this test exports a bundle, wipes the data directory, restores from the bundle, and asserts the resulting `DesiredState` byte-equals the original under canonical serialisation. Until Phase 26 is complete, this task tracks a placeholder asserting the bundle's `desired-state.json` is byte-equal to a freshly serialised desired state.
  - Done when: the placeholder test passes; the cross-phase test is captured as an open task on Phase 26.
  - Feature: T2.9.
- [ ] **Master-key wrap negative test.**
  - Acceptance: Wrong passphrase MUST yield `WrapError::Authentication`; corrupted ciphertext MUST yield the same.
  - Done when: the test passes.
  - Feature: T2.9.

### Walk-away usability

- [ ] **Author `docs/migrating-off-trilithon.md`.**
  - Path: `docs/migrating-off-trilithon.md`.
  - Acceptance: Headings: "Choosing an export format", "Pointing stock Caddy at the JSON export", "Editing the Caddyfile export by hand", "Re-importing a bundle into another Trilithon", "What you keep, what you lose", "Verifying the export against the audit log".
  - Done when: the file exists and a documentation lint asserts every heading is present.
  - Feature: T2.9 (mitigates H7).

## Cross-references

- ADR-0009 (immutable content-addressed snapshots and audit log).
- ADR-0014 (secrets vault — bundle wraps the master key).
- PRD T2.9 (configuration export).
- Architecture: "Export formats," "Bundle determinism," "Caddyfile lossiness."
- Hazards: H7 (Caddyfile escape lock-in), H10 (Secrets in audit diffs — applies to the bundle's audit-log member).

## Sign-off checklist

- [ ] `just check` passes.
- [ ] Caddy JSON export, applied to a fresh Caddy, produces identical runtime behaviour against the Phase 13 request matrix.
- [ ] Caddyfile export round-trips for every supported-subset fixture; lossy warnings are surfaced.
- [ ] Native bundle export is byte-deterministic across two consecutive invocations.
- [ ] The bundle's `master-key-wrap.bin` is present iff a passphrase was supplied; verified by `master_key_wrap_present` in the manifest.
- [ ] Every export emits exactly one audit row whose `sha256_of_artifact` matches the downloaded bytes.
- [ ] `docs/migrating-off-trilithon.md` exists with every required heading.

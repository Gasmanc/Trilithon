# Phase 26 — Backup and Restore — Implementation Slices

> Phase reference: [../phases/phase-26-backup-and-restore.md](../phases/phase-26-backup-and-restore.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Bundle format (authoritative): [bundle-format-v1.md](../architecture/bundle-format-v1.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference (`docs/phases/phase-26-backup-and-restore.md`).
- `docs/architecture/bundle-format-v1.md` — authoritative format consumed by the restore pipeline.
- Architecture §6.5 (snapshots), §6.6 (audit log), §6.9 (secrets metadata), §11 (security posture), §12.1 (tracing vocabulary), §14 (upgrade and migration; restore validates manifest before overwriting).
- Trait signatures: `core::storage::Storage` (snapshot/audit-log access for backup; atomic data-directory swap for restore), `core::secrets::SecretsVault` (master key handoff).
- ADRs: ADR-0009 (immutable content-addressed snapshots and audit log), ADR-0014 (secrets vault — master-key wrap).
- PRD: T2.12 (backup and restore).
- Hazards: H9 (Caddy version skew across snapshots — restore preflight), H10 (secrets in audit diffs).

## Slice plan summary

| # | Title | Primary files | Effort (ideal-eng-hours) | Depends on |
|---|-------|---------------|--------------------------|------------|
| 26.1 | `POST /api/v1/backup` handler with optional access-log inclusion | `core/crates/cli/src/http/backup.rs`, `core/crates/adapters/src/export/backup_packager.rs` | 4 | Phase 25 (slice 25.7) |
| 26.2 | Restore pipeline scaffolding and seven-step error type | `core/crates/core/src/restore/mod.rs`, `core/crates/adapters/src/restore/pipeline.rs` | 5 | 26.1 |
| 26.3 | Restore steps 1–2: manifest compatibility check + master-key unwrap | `core/crates/adapters/src/restore/steps/manifest.rs`, `core/crates/adapters/src/restore/steps/key.rs` | 5 | 26.2 |
| 26.4 | Restore steps 3–4: audit-log content-address validation + snapshot tree validation | `core/crates/adapters/src/restore/steps/audit.rs`, `core/crates/adapters/src/restore/steps/snapshots.rs` | 6 | 26.2 |
| 26.5 | Restore steps 5–7: preflight, atomic swap, failure leaves state untouched | `core/crates/adapters/src/restore/steps/preflight.rs`, `core/crates/adapters/src/restore/steps/swap.rs` | 7 | 26.4 |
| 26.6 | Cross-machine handoff: `installation_id` lifecycle and audit rows | `core/crates/core/src/installation_id.rs`, `core/crates/core/src/audit.rs` | 4 | 26.5 |
| 26.7 | `POST /api/v1/restore` handler and CLI subcommand | `core/crates/cli/src/http/restore.rs`, `core/crates/cli/src/commands/restore.rs` | 5 | 26.5 |
| 26.8 | Web UI Backup-and-restore page with confirmation gate | `web/src/features/backup/BackupRestorePage.tsx`, `.test.tsx` | 5 | 26.7 |

---

## Slice 26.1 — `POST /api/v1/backup` handler with optional access-log inclusion

### Goal

`POST /api/v1/backup` accepts a passphrase plus an `include_access_logs` boolean and produces the same native bundle as `POST /api/v1/export/bundle`, additionally streaming the rolling access log store into a new bundle member when the flag is set. After this slice, an integration test exercises both flag values.

### Entry conditions

- Phase 25 complete (the bundle packager exists).
- The access log store from Phase 22 exists.

### Files to create or modify

- `core/crates/cli/src/http/backup.rs` — handler.
- `core/crates/adapters/src/export/backup_packager.rs` — wraps `bundle_packager::export_bundle` with the optional access-log member.

### Signatures and shapes

```rust
//! POST /api/v1/backup
//!
//! Same artefact as POST /api/v1/export/bundle, optionally streaming
//! the rolling access log store into the bundle.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;

#[derive(serde::Deserialize)]
pub struct PostBackupBody {
    pub passphrase: String,
    #[serde(default)]
    pub include_access_logs: bool,
    #[serde(default)]
    pub allow_large_bundle: bool,
}

pub async fn post_backup(
    State(app): State<AppState>,
    Json(body): Json<PostBackupBody>,
) -> Result<Response, BackupHttpError> {
    let mut buffer = Vec::new();
    trilithon_adapters::export::backup_packager::pack_backup(
        app.storage(),
        app.access_log_store(),
        &trilithon_adapters::export::backup_packager::BackupRequest {
            passphrase: &body.passphrase,
            include_access_logs: body.include_access_logs,
        },
        &mut buffer,
    ).await?;
    if !body.allow_large_bundle && buffer.len() > 256 * 1024 * 1024 {
        return Err(BackupHttpError::TooLarge { actual: buffer.len() });
    }
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/gzip"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"trilithon-backup.tar.gz\""),
    );
    Ok((StatusCode::OK, headers, Body::from(buffer)).into_response())
}
```

```rust
// core/crates/adapters/src/export/backup_packager.rs
//!
//! Backup packager. Delegates to the Phase 25 bundle packager and,
//! when requested, appends an `access-logs.ndjson` member containing
//! the contents of the rolling access log store.

pub struct BackupRequest<'a> {
    pub passphrase: &'a str,
    pub include_access_logs: bool,
}

pub async fn pack_backup<W: std::io::Write>(
    storage: &dyn trilithon_core::storage::Storage,
    access_log: &dyn crate::access_log::AccessLogStore,
    request: &BackupRequest<'_>,
    output: W,
) -> Result<(), BackupExportError> {
    // Implementation: build the Phase 25 BundleExportRequest with
    // include_secrets = true; if include_access_logs is true, capture
    // an access-logs.ndjson stream and inject it as an extra TarMember
    // in the bundle pre-pack assembly.
    todo!("implementation")
}

#[derive(Debug, thiserror::Error)]
pub enum BackupExportError {
    #[error(transparent)]
    Bundle(#[from] crate::export::bundle_packager::BundleExportError),
    #[error("access log: {0}")]
    AccessLog(String),
}
```

### Algorithm

1. Construct a `BundleExportRequest` from Phase 25 with `include_secrets = true` and the supplied passphrase.
2. If `include_access_logs`, query the access log store for every retained line and assemble an `access-logs.ndjson` `TarMember`. Otherwise skip.
3. Drive `bundle_packager::export_bundle` with the optional extra member.
4. Stream the resulting archive bytes into `output`.

### Tests

- `http_backup::tests::without_access_logs_omits_member` — `include_access_logs = false`; extract the archive; assert no `access-logs.ndjson` member.
- `http_backup::tests::with_access_logs_includes_member` — populate access log store; `include_access_logs = true`; assert the member is present and lines round-trip.
- `http_backup::tests::respects_size_cap` — without `allow_large_bundle`; assert 413 over 256 MiB.

### Acceptance command

```
cargo test -p trilithon-cli http_backup
```

### Exit conditions

- The endpoint exists and the three tests pass.
- The bundle output is byte-deterministic when `include_access_logs = false`.

### Audit kinds emitted

- `export.bundle` (architecture §6.6) — one row per backup, with `notes.format = "bundle"` and `notes.sha256_of_artifact` matching the response bytes.

### Tracing events emitted

- `http.request.received`, `http.request.completed` (architecture §12.1).

### Cross-references

- ADR-0014.
- PRD T2.12.
- Architecture §6.6.

---

## Slice 26.2 — Restore pipeline scaffolding and seven-step error type

### Goal

A typed, sequential pipeline at `core::restore` and `adapters::restore::pipeline` represents the seven restore steps as enum variants of a `RestoreError` and a `RestorePlan` orchestrator that runs them in order. After this slice, a stub `restore` flow runs the seven steps against a no-op implementation and returns success.

### Entry conditions

- Slice 25.7 complete (bundle packer / parser shapes available).

### Files to create or modify

- `core/crates/core/src/restore/mod.rs` — pure pipeline shapes.
- `core/crates/adapters/src/restore/pipeline.rs` — adapter-side orchestration.

### Signatures and shapes

```rust
//! Restore pipeline shapes.
//!
//! Phase reference: docs/phases/phase-26-backup-and-restore.md.
//! Authoritative bundle format: docs/architecture/bundle-format-v1.md.

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("manifest incompatible: {detail}")]
    ManifestIncompatible { detail: String },
    #[error("authentication failed (wrong passphrase or corrupt master-key wrap)")]
    Authentication,
    #[error("audit log content-address mismatch at row {row_id}")]
    AuditLogTampered { row_id: String },
    #[error("snapshot tree invalid: {detail}")]
    SnapshotTreeInvalid { detail: String },
    #[error("preflight reported errors: {detail}")]
    PreflightFailed { detail: String },
    #[error("atomic swap failed: {detail}; staging directory at {staging}")]
    AtomicSwapFailed { detail: String, staging: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Storage(#[from] crate::storage::StorageError),
}

#[derive(Debug, Clone)]
pub struct RestoreOutcome {
    pub source_installation_id: String,
    pub new_installation_id: String,
    pub root_snapshot_id: String,
    pub preflight_warnings: Vec<String>,
}
```

```rust
// core/crates/adapters/src/restore/pipeline.rs
//!
//! Seven-step restore pipeline.
//!
//! 1. Verify manifest against compatibility matrix.
//! 2. Decrypt the master-key wrap.
//! 3. Validate audit-log content-address.
//! 4. Validate snapshot tree.
//! 5. Run preflight; warnings only (H9).
//! 6. Atomic swap on full pass.
//! 7. On any failure leave existing state untouched.

use trilithon_core::restore::{RestoreError, RestoreOutcome};

pub struct RestoreRequest<'a> {
    pub bundle_bytes: &'a [u8],
    pub passphrase: &'a str,
}

pub struct RestoreOptions {
    pub data_dir: std::path::PathBuf,
    pub staging_root: std::path::PathBuf,
}

pub async fn restore_bundle(
    request: &RestoreRequest<'_>,
    options: &RestoreOptions,
    storage: &dyn trilithon_core::storage::Storage,
    secrets: &dyn trilithon_core::secrets::SecretsVault,
) -> Result<RestoreOutcome, RestoreError> {
    let parsed = parse_bundle(request.bundle_bytes)?;
    super::steps::manifest::verify(&parsed.manifest)?;
    let master_key = super::steps::key::unwrap(&parsed, request.passphrase)?;
    super::steps::audit::validate_content_addresses(&parsed)?;
    super::steps::snapshots::validate_tree(&parsed)?;
    let warnings = super::steps::preflight::run(&parsed, storage).await?;
    super::steps::swap::atomic_swap(&parsed, &master_key, options).await?;
    let outcome = super::steps::installation_id::record_cross_machine(
        &parsed.manifest, storage,
    ).await?;
    Ok(RestoreOutcome {
        source_installation_id: parsed.manifest.source_installation_id,
        new_installation_id: outcome.new_installation_id,
        root_snapshot_id: parsed.manifest.root_snapshot_id,
        preflight_warnings: warnings,
    })
}

struct ParsedBundle { /* manifest, members, master_key_wrap, ... */ }
fn parse_bundle(_bytes: &[u8]) -> Result<ParsedBundle, RestoreError> { todo!() }
```

### Algorithm

1. Parse the archive bytes into the structured `ParsedBundle` (manifest, snapshot files, audit log, secrets blob, master-key wrap).
2. Run each step in sequence; on any error return the typed variant.
3. On full pass record the cross-machine audit row and return `RestoreOutcome`.

### Tests

- `restore::pipeline::tests::happy_path_calls_all_seven_steps` — using doubles for each step, assert all seven are invoked in order.
- `restore::pipeline::tests::error_at_step_1_returns_typed_error` — manifest-incompatible bundle; assert `RestoreError::ManifestIncompatible`.
- `restore::pipeline::tests::error_at_step_3_does_not_call_step_4_or_later` — tamper bundle; assert short-circuit.

### Acceptance command

```
cargo test -p trilithon-adapters restore::pipeline
```

### Exit conditions

- The seven-step orchestrator exists.
- The error type covers every documented failure branch.

### Audit kinds emitted

None at this layer; the per-step modules emit their own audit rows where appropriate (slice 26.5 records `RestoreApplied`, slice 26.6 records cross-machine).

### Tracing events emitted

None at this layer.

### Cross-references

- ADR-0009, ADR-0014.
- `bundle-format-v1.md`.
- PRD T2.12.

---

## Slice 26.3 — Restore steps 1–2: manifest compatibility + master-key unwrap

### Goal

Step 1 verifies the bundle manifest's `schema_version` against the local Trilithon's compatibility matrix, refusing future schemas. Step 2 decrypts the master-key wrap with the user-supplied passphrase. Either step's failure halts the pipeline before any state is touched.

### Entry conditions

- Slice 26.2 complete.
- Slice 25.4 (manifest type) and 25.6 (master-key unwrap) complete.

### Files to create or modify

- `core/crates/adapters/src/restore/steps/manifest.rs` — step 1.
- `core/crates/adapters/src/restore/steps/key.rs` — step 2.

### Signatures and shapes

```rust
// core/crates/adapters/src/restore/steps/manifest.rs
use trilithon_core::export::manifest::BundleManifest;
use trilithon_core::restore::RestoreError;

/// Compatibility matrix:
///
/// - `schema_version == 1`  → accepted unconditionally on V1.x.
/// - `schema_version > 1`   → rejected with `ManifestIncompatible`.
/// - `schema_version == 0` or any other value → rejected.
pub fn verify(manifest: &BundleManifest) -> Result<(), RestoreError> {
    match manifest.schema_version {
        1 => Ok(()),
        v if v > 1 => Err(RestoreError::ManifestIncompatible {
            detail: format!(
                "bundle schema_version {v} is newer than this Trilithon's max (1); \
                 upgrade Trilithon and retry"
            ),
        }),
        v => Err(RestoreError::ManifestIncompatible {
            detail: format!("bundle schema_version {v} is invalid"),
        }),
    }
}
```

```rust
// core/crates/adapters/src/restore/steps/key.rs
use trilithon_core::restore::RestoreError;

use crate::export::master_key_wrap::{unwrap_master_key, WrapError};

pub fn unwrap(
    parsed: &super::super::pipeline::ParsedBundle,
    passphrase: &str,
) -> Result<[u8; 32], RestoreError> {
    let envelope = parsed.master_key_wrap_bytes()
        .ok_or_else(|| RestoreError::ManifestIncompatible {
            detail: "bundle has no master-key-wrap.bin; cross-machine restore \
                     requires a passphrase-protected bundle".into(),
        })?;
    match unwrap_master_key(envelope, passphrase) {
        Ok(key) => Ok(key),
        Err(WrapError::Authentication) => Err(RestoreError::Authentication),
        Err(other) => Err(RestoreError::ManifestIncompatible {
            detail: format!("master-key wrap unparseable: {other}"),
        }),
    }
}
```

### Algorithm

`verify`:

1. Read `manifest.schema_version`.
2. Accept `1`. Reject anything greater (with an upgrade message). Reject anything else.

`unwrap`:

1. Pull the wrap bytes from the parsed bundle. If absent, refuse.
2. Call `unwrap_master_key`.
3. Map `WrapError::Authentication` → `RestoreError::Authentication`. Map other wrap errors → `ManifestIncompatible`.

### Tests

- `restore::steps::manifest::tests::accepts_schema_v1`.
- `restore::steps::manifest::tests::rejects_future_schema` — schema 2; expect upgrade message in detail.
- `restore::steps::manifest::tests::rejects_invalid_schema_zero`.
- `restore::steps::key::tests::wrong_passphrase_returns_authentication` — matches phase-reference acceptance.
- `restore::steps::key::tests::missing_wrap_returns_manifest_incompatible`.

### Acceptance command

```
cargo test -p trilithon-adapters restore::steps::manifest \
  && cargo test -p trilithon-adapters restore::steps::key
```

### Exit conditions

- Both step modules exist.
- Wrong-passphrase test passes (an integration assertion the phase reference requires).
- Future-schema bundle is typed-rejected.

### Audit kinds emitted

None at this layer; the orchestrator records the eventual outcome.

### Tracing events emitted

None at this layer.

### Cross-references

- ADR-0014.
- Architecture §14 (upgrade and migration).

---

## Slice 26.4 — Restore steps 3–4: audit-log content-address + snapshot tree validation

### Goal

Step 3 walks every line of `audit-log.ndjson` and verifies the row's content-address against its `id`. Step 4 walks the `snapshots/` directory: every parent pointer MUST resolve to a present snapshot, every snapshot's content hash MUST equal its filename stem.

### Entry conditions

- Slice 26.3 complete.

### Files to create or modify

- `core/crates/adapters/src/restore/steps/audit.rs`.
- `core/crates/adapters/src/restore/steps/snapshots.rs`.

### Signatures and shapes

```rust
// core/crates/adapters/src/restore/steps/audit.rs
use trilithon_core::restore::RestoreError;

pub fn validate_content_addresses(
    parsed: &super::super::pipeline::ParsedBundle,
) -> Result<(), RestoreError> {
    for line in parsed.audit_log_lines() {
        let row: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| RestoreError::AuditLogTampered { row_id: format!("(parse) {e}") })?;
        let id = row.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| RestoreError::AuditLogTampered { row_id: "(missing id)".into() })?;
        let canonical = canonical_audit_payload(&row);
        let computed = lowercase_hex_sha256(&canonical);
        if computed != id {
            return Err(RestoreError::AuditLogTampered { row_id: id.to_string() });
        }
    }
    Ok(())
}

fn canonical_audit_payload(row: &serde_json::Value) -> Vec<u8> {
    // The audit row's content-address is the SHA-256 of its canonical
    // JSON serialisation EXCLUDING the `id` field.
    let mut copy = row.clone();
    if let Some(obj) = copy.as_object_mut() {
        obj.remove("id");
    }
    let mut buf = Vec::new();
    trilithon_core::export::deterministic::write_canonical_compact(&mut buf, &copy)
        .expect("canonical writer infallible on Vec<u8>");
    buf
}

fn lowercase_hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
```

```rust
// core/crates/adapters/src/restore/steps/snapshots.rs
use std::collections::HashMap;
use trilithon_core::restore::RestoreError;

pub fn validate_tree(
    parsed: &super::super::pipeline::ParsedBundle,
) -> Result<(), RestoreError> {
    let snapshots: HashMap<String, &[u8]> = parsed.snapshot_files();

    for (id, body) in &snapshots {
        let computed = super::audit::lowercase_hex_sha256(body);
        if &computed != id {
            return Err(RestoreError::SnapshotTreeInvalid {
                detail: format!("snapshot {id} content hash mismatch"),
            });
        }
        let parsed: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| RestoreError::SnapshotTreeInvalid {
                detail: format!("snapshot {id} not valid JSON: {e}"),
            })?;
        if let Some(parent) = parsed.get("parent_id").and_then(|v| v.as_str()) {
            if !snapshots.contains_key(parent) {
                return Err(RestoreError::SnapshotTreeInvalid {
                    detail: format!("snapshot {id} references missing parent {parent}"),
                });
            }
        }
    }
    Ok(())
}
```

### Algorithm

`validate_content_addresses`: for each audit line, recompute its hash from canonical JSON minus the `id` field, compare to the `id` field; mismatch → `AuditLogTampered`.

`validate_tree`: for each snapshot file, hash the body, compare to filename stem; for each `parent_id`, assert reachable in the bundle.

### Tests

- `restore::steps::audit::tests::tampered_row_rejected` — flip a byte in `before_redacted`; assert `AuditLogTampered`.
- `restore::steps::audit::tests::clean_log_passes`.
- `restore::steps::snapshots::tests::missing_parent_rejected`.
- `restore::steps::snapshots::tests::content_mismatch_rejected`.
- `restore::steps::snapshots::tests::clean_chain_passes`.

### Acceptance command

```
cargo test -p trilithon-adapters restore::steps::audit \
  && cargo test -p trilithon-adapters restore::steps::snapshots
```

### Exit conditions

- Both step modules pass their named tests.
- Tampered logs and corrupt chains are rejected before any state mutation.

### Audit kinds emitted

None at this layer.

### Tracing events emitted

None at this layer.

### Cross-references

- ADR-0009.
- `bundle-format-v1.md` §6, §7.

---

## Slice 26.5 — Restore steps 5–7: preflight, atomic swap, failure leaves state untouched

### Goal

Step 5 runs preflight against the post-restore desired state; failures surface as warnings (per H9) rather than blockers. Step 6 atomically swaps the data directory under an exclusive lock. Step 7 ensures any failure in steps 1–6 leaves the existing state untouched and the staging directory available for forensic inspection.

### Entry conditions

- Slices 26.3 and 26.4 complete.
- The preflight engine (Phase 12) is available.

### Files to create or modify

- `core/crates/adapters/src/restore/steps/preflight.rs`.
- `core/crates/adapters/src/restore/steps/swap.rs`.

### Signatures and shapes

```rust
// core/crates/adapters/src/restore/steps/preflight.rs
use trilithon_core::restore::RestoreError;

pub async fn run(
    parsed: &super::super::pipeline::ParsedBundle,
    storage: &dyn trilithon_core::storage::Storage,
) -> Result<Vec<String>, RestoreError> {
    // Run the Phase 12 preflight engine against the parsed bundle's
    // desired state. Per hazard H9: failures are warnings, not errors.
    let report = trilithon_core::preflight::run_against(&parsed.desired_state(), storage).await
        .map_err(|e| RestoreError::PreflightFailed { detail: e.to_string() })?;
    Ok(report.warnings_human_readable())
}
```

```rust
// core/crates/adapters/src/restore/steps/swap.rs
use std::path::{Path, PathBuf};

use trilithon_core::restore::RestoreError;

pub async fn atomic_swap(
    parsed: &super::super::pipeline::ParsedBundle,
    master_key: &[u8; 32],
    options: &super::super::pipeline::RestoreOptions,
) -> Result<(), RestoreError> {
    let staging = options.staging_root.join(format!(
        "restore-staging-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
    ));
    std::fs::create_dir_all(&staging)?;

    write_staging(&staging, parsed, master_key)?;

    let lock_path = options.data_dir.join(".restore.lock");
    let lock = exclusive_lock(&lock_path)?;

    let backup_dir = staging.join("previous-data-dir");
    if options.data_dir.exists() {
        std::fs::rename(&options.data_dir, &backup_dir)
            .map_err(|e| RestoreError::AtomicSwapFailed {
                detail: format!("renaming previous data dir: {e}"),
                staging: staging.display().to_string(),
            })?;
    }

    if let Err(e) = std::fs::rename(staging.join("data"), &options.data_dir) {
        // Roll back: restore the previous data dir.
        if backup_dir.exists() {
            let _ = std::fs::rename(&backup_dir, &options.data_dir);
        }
        return Err(RestoreError::AtomicSwapFailed {
            detail: format!("renaming staged data into place: {e}"),
            staging: staging.display().to_string(),
        });
    }

    drop(lock);
    Ok(())
}

fn write_staging(_staging: &Path, _parsed: &super::super::pipeline::ParsedBundle, _key: &[u8; 32])
    -> Result<(), RestoreError>
{ todo!() }

fn exclusive_lock(_path: &Path) -> Result<std::fs::File, RestoreError> { todo!() }
```

### Algorithm

`preflight::run`: drive the Phase 12 preflight engine against the bundle's desired state; return its warnings list. Per H9, certificate-validity and Caddy-version-skew issues are warnings.

`swap::atomic_swap`:

1. Create a staging directory under `options.staging_root`.
2. Materialise the restored data (SQLite, secrets blob, snapshots cache) into `staging/data/`.
3. Acquire an exclusive `flock` on `data_dir/.restore.lock`.
4. Rename `data_dir → staging/previous-data-dir`.
5. Rename `staging/data → data_dir`.
6. On any failure inside steps 4–5, roll back: rename `staging/previous-data-dir` back to `data_dir`. Return `AtomicSwapFailed` with the staging path so an operator can inspect.
7. Release the lock.

### Tests

- `restore::steps::swap::tests::happy_path_swaps_data_dir`.
- `restore::steps::swap::tests::failure_during_swap_rolls_back` — inject an `EACCES` on the second rename; assert the original data dir is restored and `AtomicSwapFailed` is returned with the staging path.
- `restore::steps::swap::tests::staging_directory_persists_on_failure` — assert the staging dir is NOT cleaned up so an operator can inspect.
- `restore::steps::preflight::tests::caddy_version_skew_returns_warning_not_error` — bundle records `caddy_version = 2.8.4`, local Caddy is `2.11.2`; assert preflight returns a warning, not a `RestoreError`.

### Acceptance command

```
cargo test -p trilithon-adapters restore::steps::swap \
  && cargo test -p trilithon-adapters restore::steps::preflight
```

### Exit conditions

- Atomic swap succeeds on the happy path.
- Failure during swap rolls back and preserves the staging directory.
- Preflight surfaces Caddy-version skew as a warning per H9.

### Audit kinds emitted

After step 6 succeeds, the orchestrator records:

- `system.restore-applied` — pending; this kind MUST be added to architecture §6.6 in the same commit (per the §6.6 vocabulary-authority rule). The slice's documentation ticket records the addition.

(NOTE: the phase reference uses the wire string corresponding to the Rust variant `RestoreApplied`. Whether this maps to a new `system.restore-applied` kind or to an existing kind is an open question; see Open questions below.)

### Tracing events emitted

None new; existing `apply.started` / `apply.succeeded` events from the reconciler fire when the post-restore reconciler tick runs.

### Cross-references

- ADR-0009.
- Architecture §14 (upgrade and migration).
- Hazards: H9.

---

## Slice 26.6 — Cross-machine handoff: `installation_id` lifecycle and audit rows

### Goal

The bundle manifest carries the source `installation_id`. Restoring on a different machine produces a new `installation_id` and writes a `RestoreCrossMachine` audit row recording both identifiers.

### Entry conditions

- Slice 26.5 complete.

### Files to create or modify

- `core/crates/core/src/installation_id.rs` — the type and persistence boundary.
- `core/crates/core/src/audit.rs` — add `AuditEvent::RestoreApplied` and `AuditEvent::RestoreCrossMachine` variants and update §6.6 in the same commit.
- `core/crates/adapters/src/restore/steps/installation_id.rs` — handoff logic.

### Signatures and shapes

```rust
// core/crates/core/src/installation_id.rs
//! Stable per-installation identifier used by the cross-machine
//! restore path to correlate the source bundle with the target
//! Trilithon instance.

use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct InstallationId(String);

impl InstallationId {
    pub fn new_random() -> Self {
        InstallationId(ulid::Ulid::new().to_string())
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl FromStr for InstallationId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ulid::Ulid::from_string(s).map_err(|e| e.to_string())?;
        Ok(InstallationId(s.to_string()))
    }
}
```

```rust
// core/crates/core/src/audit.rs (relevant additions)
#[derive(Debug, Clone)]
pub enum AuditEvent {
    // ...
    RestoreApplied { source_installation_id: String, root_snapshot_id: String },
    RestoreCrossMachine { source_installation_id: String, new_installation_id: String },
}

impl std::fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // ...
            AuditEvent::RestoreApplied { .. }       => f.write_str("system.restore-applied"),
            AuditEvent::RestoreCrossMachine { .. }  => f.write_str("system.restore-cross-machine"),
        }
    }
}
```

```rust
// core/crates/adapters/src/restore/steps/installation_id.rs
use trilithon_core::installation_id::InstallationId;
use trilithon_core::restore::RestoreError;

pub struct InstallationOutcome { pub new_installation_id: String }

pub async fn record_cross_machine(
    manifest: &trilithon_core::export::manifest::BundleManifest,
    storage: &dyn trilithon_core::storage::Storage,
) -> Result<InstallationOutcome, RestoreError> {
    let local = storage.read_or_create_installation_id().await?;
    let restore_applied = trilithon_core::audit::AuditEvent::RestoreApplied {
        source_installation_id: manifest.source_installation_id.clone(),
        root_snapshot_id: manifest.root_snapshot_id.clone(),
    };
    storage.record_audit_event(audit_row_from(&restore_applied)).await?;

    if local.as_str() != manifest.source_installation_id {
        let new_id = InstallationId::new_random();
        storage.write_installation_id(&new_id).await?;
        storage.record_audit_event(audit_row_from(
            &trilithon_core::audit::AuditEvent::RestoreCrossMachine {
                source_installation_id: manifest.source_installation_id.clone(),
                new_installation_id: new_id.as_str().to_string(),
            },
        )).await?;
        return Ok(InstallationOutcome { new_installation_id: new_id.as_str().to_string() });
    }
    Ok(InstallationOutcome { new_installation_id: local.as_str().to_string() })
}

fn audit_row_from(_event: &trilithon_core::audit::AuditEvent)
    -> trilithon_core::storage::AuditEventRow { todo!() }
```

### Algorithm

1. Read the local `installation_id` from storage (creating one if absent).
2. Always record `RestoreApplied` for the restore.
3. If the local id differs from the manifest's source id, generate a fresh `installation_id` and record `RestoreCrossMachine` referencing both ids.

### Tests

- `restore::steps::installation_id::tests::same_machine_no_cross_machine_row`.
- `restore::steps::installation_id::tests::different_machine_writes_cross_machine_row` — assert both ids appear in the row.
- `audit::tests::restore_applied_kind_string` — `"system.restore-applied"`.
- `audit::tests::restore_cross_machine_kind_string` — `"system.restore-cross-machine"`.

### Acceptance command

```
cargo test -p trilithon-adapters restore::steps::installation_id \
  && cargo test -p trilithon-core audit::tests::restore
```

### Exit conditions

- The `installation_id` type and persistence path exist.
- Same-machine restore writes only `RestoreApplied`.
- Cross-machine restore writes both rows; the cross-machine row contains both ids.
- Architecture §6.6 is updated in the same commit to add `system.restore-applied` and `system.restore-cross-machine`.

### Audit kinds emitted

- `system.restore-applied` (added to architecture §6.6 in this slice's commit).
- `system.restore-cross-machine` (added to architecture §6.6 in this slice's commit).

### Tracing events emitted

None new.

### Cross-references

- ADR-0009.
- Architecture §6.6 (must be updated in the same commit).

---

## Slice 26.7 — `POST /api/v1/restore` handler and CLI subcommand

### Goal

`POST /api/v1/restore` accepts a multipart upload (the bundle bytes plus the passphrase) and runs the seven-step pipeline. A CLI subcommand `trilithon restore --bundle <path> --passphrase-stdin` exposes the same pipeline for offline operators.

### Entry conditions

- Slices 26.2 through 26.6 complete.

### Files to create or modify

- `core/crates/cli/src/http/restore.rs` — handler.
- `core/crates/cli/src/commands/restore.rs` — CLI subcommand.

### Signatures and shapes

```rust
// core/crates/cli/src/http/restore.rs
use axum::extract::{Multipart, State};
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(serde::Serialize)]
pub struct RestoreResponse {
    pub source_installation_id: String,
    pub new_installation_id: String,
    pub root_snapshot_id: String,
    pub preflight_warnings: Vec<String>,
}

pub async fn post_restore(
    State(app): State<AppState>,
    mut multipart: Multipart,
) -> Result<Response, RestoreHttpError> {
    let mut bundle = None;
    let mut passphrase = None;
    while let Some(field) = multipart.next_field().await? {
        match field.name() {
            Some("bundle") => { bundle = Some(field.bytes().await?.to_vec()); }
            Some("passphrase") => { passphrase = Some(field.text().await?); }
            _ => {}
        }
    }
    let bundle = bundle.ok_or(RestoreHttpError::MissingBundle)?;
    let passphrase = passphrase.ok_or(RestoreHttpError::MissingPassphrase)?;

    let outcome = trilithon_adapters::restore::pipeline::restore_bundle(
        &trilithon_adapters::restore::pipeline::RestoreRequest {
            bundle_bytes: &bundle,
            passphrase: &passphrase,
        },
        &trilithon_adapters::restore::pipeline::RestoreOptions {
            data_dir: app.data_dir().to_path_buf(),
            staging_root: app.staging_root().to_path_buf(),
        },
        app.storage(),
        app.secrets(),
    ).await?;
    Ok((StatusCode::OK, Json(RestoreResponse {
        source_installation_id: outcome.source_installation_id,
        new_installation_id: outcome.new_installation_id,
        root_snapshot_id: outcome.root_snapshot_id,
        preflight_warnings: outcome.preflight_warnings,
    })).into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreHttpError {
    #[error("multipart: {0}")]
    Multipart(#[from] axum::extract::multipart::MultipartError),
    #[error("missing bundle field")]
    MissingBundle,
    #[error("missing passphrase field")]
    MissingPassphrase,
    #[error(transparent)]
    Restore(#[from] trilithon_core::restore::RestoreError),
}

impl IntoResponse for RestoreHttpError {
    fn into_response(self) -> Response {
        use trilithon_core::restore::RestoreError;
        let (status, body) = match &self {
            Self::Restore(RestoreError::Authentication) =>
                (StatusCode::UNAUTHORIZED, "wrong passphrase"),
            Self::Restore(RestoreError::ManifestIncompatible { .. }) =>
                (StatusCode::CONFLICT, "manifest incompatible"),
            Self::Restore(RestoreError::AuditLogTampered { .. }) =>
                (StatusCode::UNPROCESSABLE_ENTITY, "audit log tampered"),
            Self::Restore(RestoreError::SnapshotTreeInvalid { .. }) =>
                (StatusCode::UNPROCESSABLE_ENTITY, "snapshot tree invalid"),
            Self::Restore(RestoreError::PreflightFailed { .. }) =>
                (StatusCode::INTERNAL_SERVER_ERROR, "preflight failed"),
            Self::Restore(RestoreError::AtomicSwapFailed { .. }) =>
                (StatusCode::INTERNAL_SERVER_ERROR, "atomic swap failed"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        (status, Json(serde_json::json!({
            "error": body, "detail": self.to_string()
        }))).into_response()
    }
}
```

```rust
// core/crates/cli/src/commands/restore.rs
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, clap::Args)]
pub struct RestoreArgs {
    #[arg(long)]
    pub bundle: PathBuf,
    #[arg(long)]
    pub passphrase_stdin: bool,
}

pub async fn run(args: RestoreArgs) -> ExitCode {
    let bytes = match std::fs::read(&args.bundle) {
        Ok(b)  => b,
        Err(e) => { eprintln!("trilithon restore: reading {}: {e}", args.bundle.display());
                    return ExitCode::FAILURE; }
    };
    let mut passphrase = String::new();
    if args.passphrase_stdin {
        if std::io::stdin().read_to_string(&mut passphrase).is_err() {
            eprintln!("trilithon restore: failed to read passphrase from stdin");
            return ExitCode::FAILURE;
        }
    }
    let trimmed = passphrase.trim_end_matches('\n');
    // Drive trilithon_adapters::restore::pipeline::restore_bundle in-process.
    todo!("wire pipeline call and print outcome")
}
```

### Algorithm

HTTP handler:

1. Parse multipart fields `bundle` and `passphrase`.
2. Drive `restore_bundle`.
3. Map typed errors to HTTP statuses.

CLI:

1. Read bundle bytes from disk.
2. Read passphrase from stdin if requested.
3. Drive `restore_bundle` in-process.

### Tests

- `core/crates/cli/tests/http_restore_happy_path.rs` — upload a known-good bundle; assert 200 and the response contains the expected ids.
- `core/crates/cli/tests/http_restore_wrong_passphrase.rs` — assert 401.
- `core/crates/cli/tests/http_restore_tampered_log.rs` — assert 422 with `"audit log tampered"`.
- `core/crates/cli/tests/http_restore_future_schema.rs` — assert 409.
- `core/crates/cli/tests/cli_restore_happy_path.rs` — invoke the binary with a bundle file; assert exit 0.

### Acceptance command

```
cargo test -p trilithon-cli http_restore \
  && cargo test -p trilithon-cli cli_restore
```

### Exit conditions

- Both surfaces exist and pass their tests.
- Each typed `RestoreError` maps to a documented HTTP status.

### Audit kinds emitted

- `system.restore-applied`, optionally `system.restore-cross-machine` (slice 26.6).

### Tracing events emitted

- `http.request.received`, `http.request.completed` (architecture §12.1).

### Cross-references

- PRD T2.12.

---

## Slice 26.8 — Web UI Backup-and-restore page with confirmation gate

### Goal

A React page at `web/src/features/backup/BackupRestorePage.tsx` hosts a "Create backup" form (passphrase, optional include-logs flag) and a "Restore from bundle" form (file upload, passphrase, explicit confirmation). After this slice, Vitest tests cover both flows including the confirmation gate.

### Entry conditions

- Slice 26.7 complete.

### Files to create or modify

- `web/src/features/backup/BackupRestorePage.tsx`.
- `web/src/features/backup/BackupRestorePage.test.tsx`.

### Signatures and shapes

```tsx
import { useState } from 'react';

interface BackupRestorePageProps {
  readonly onCreateBackup: (input: {
    passphrase: string;
    includeAccessLogs: boolean;
  }) => Promise<void>;
  readonly onRestore: (input: {
    bundle: File;
    passphrase: string;
  }) => Promise<{
    sourceInstallationId: string;
    newInstallationId: string;
    preflightWarnings: readonly string[];
  }>;
}

export function BackupRestorePage(props: BackupRestorePageProps): JSX.Element {
  const [createPassphrase, setCreatePassphrase] = useState<string>('');
  const [includeAccessLogs, setIncludeAccessLogs] = useState<boolean>(false);

  const [restoreFile, setRestoreFile] = useState<File | null>(null);
  const [restorePassphrase, setRestorePassphrase] = useState<string>('');
  const [restoreConfirmed, setRestoreConfirmed] = useState<boolean>(false);

  return (
    <main>
      <h1>Backup and restore</h1>

      <section aria-labelledby="backup-form-title">
        <h2 id="backup-form-title">Create backup</h2>
        <label>
          Passphrase
          <input
            type="password"
            value={createPassphrase}
            onChange={(e) => setCreatePassphrase(e.target.value)}
          />
        </label>
        <label>
          <input
            type="checkbox"
            checked={includeAccessLogs}
            onChange={(e) => setIncludeAccessLogs(e.target.checked)}
          />
          Include access logs (larger archive)
        </label>
        <button
          type="button"
          disabled={createPassphrase.length === 0}
          onClick={() => {
            void props.onCreateBackup({
              passphrase: createPassphrase,
              includeAccessLogs,
            });
          }}
        >
          Create backup
        </button>
      </section>

      <section aria-labelledby="restore-form-title">
        <h2 id="restore-form-title">Restore from bundle</h2>
        <label>
          Bundle file
          <input
            type="file"
            accept=".tar.gz,application/gzip"
            onChange={(e) => setRestoreFile(e.target.files?.[0] ?? null)}
          />
        </label>
        <label>
          Passphrase
          <input
            type="password"
            value={restorePassphrase}
            onChange={(e) => setRestorePassphrase(e.target.value)}
          />
        </label>
        <label>
          <input
            type="checkbox"
            checked={restoreConfirmed}
            onChange={(e) => setRestoreConfirmed(e.target.checked)}
          />
          I understand this will overwrite the local desired state.
        </label>
        <button
          type="button"
          disabled={
            restoreFile === null
            || restorePassphrase.length === 0
            || !restoreConfirmed
          }
          onClick={() => {
            if (restoreFile !== null) {
              void props.onRestore({
                bundle: restoreFile,
                passphrase: restorePassphrase,
              });
            }
          }}
        >
          Restore
        </button>
      </section>
    </main>
  );
}
```

### Algorithm

1. Render two sections: Create backup, Restore from bundle.
2. The create button is disabled until a passphrase is entered.
3. The restore button is disabled until a bundle file is selected, a passphrase entered, and the explicit confirmation checkbox ticked.
4. Calls into `onCreateBackup` and `onRestore` props.

### Tests

- `BackupRestorePage.test.tsx`:
  - `disables Create backup until passphrase entered`.
  - `disables Restore until file, passphrase, and confirmation all present`.
  - `Create backup invokes onCreateBackup with the entered values`.
  - `Restore invokes onRestore with the file and passphrase`.
  - `unticking confirmation re-disables Restore`.

### Acceptance command

```
pnpm vitest run web/src/features/backup/BackupRestorePage.test.tsx
```

### Exit conditions

- The page exists with both forms.
- The confirmation gate works (button disabled until confirmation checkbox ticked).
- All five Vitest cases pass.

### Audit kinds emitted

The page does not emit audit rows; it triggers HTTP calls handled by slices 26.1 and 26.7.

### Tracing events emitted

None at the React layer.

### Cross-references

- PRD T2.12.

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Backups are encrypted with a user-chosen passphrase (slices 26.1 and 25.6 — master-key wrap).
- [ ] Restore validates the backup before overwriting any state (slices 26.3 through 26.5).
- [ ] Restore on a different machine produces an identical desired state and an audit log entry recording the restore (slice 26.6).
- [ ] A tampered bundle is rejected at audit-log validation (slice 26.4).
- [ ] A wrong-passphrase bundle is rejected at master-key unwrap (slice 26.3).
- [ ] A future-schema bundle is rejected at manifest verification (slice 26.3).
- [ ] On any failure the data directory is untouched and the staging directory is preserved for forensic inspection (slice 26.5).
- [ ] Architecture §6.6 includes `system.restore-applied` and `system.restore-cross-machine` (slice 26.6).

## Open questions

1. The wire `kind` strings for `RestoreApplied` and `RestoreCrossMachine` are NOT currently in the architecture §6.6 vocabulary table. The slice 26.6 commit MUST add them. The names `system.restore-applied` and `system.restore-cross-machine` are proposals consistent with the §6.6 naming convention; ratification by the §6.6 vocabulary authority is required before the commit lands.

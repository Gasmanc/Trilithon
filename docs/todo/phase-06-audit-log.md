# Phase 06 — Audit log with secrets-aware redactor — Implementation Slices

> Phase reference: [../phases/phase-06-audit-log.md](../phases/phase-06-audit-log.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [../phases/phase-06-audit-log.md](../phases/phase-06-audit-log.md).
- Architecture §4.1 (`core` crate boundary), §4.2 (`adapters` crate boundary), §6.6 (`audit_log` table and the audit `kind` vocabulary), §7.1 (mutation lifecycle, audit append points), §12 (observability) and §12.1 (tracing vocabulary).
- Trait signatures: `core::storage::Storage` (the `record_audit_event` and `tail_audit_log` methods), `core::secrets::SecretsVault::redact`, `core::diff::DiffEngine::redact_diff`.
- ADRs: ADR-0009 (immutable content-addressed snapshots and audit log), ADR-0014 (secrets encrypted at rest with keychain master key).

## Slice plan summary

| # | Title | Primary files | Effort (h) | Depends on |
|---|-------|---------------|-----------:|-----------|
| 6.1 | `AuditEvent` enum and Display-to-wire mapping in `core` | `core/crates/core/src/audit/event.rs` | 4 | Phase 5 complete |
| 6.2 | `AuditEventRow` record, `AuditSelector`, `AuditOutcome`, `ActorRef` types | `core/crates/core/src/audit/row.rs` | 3 | 6.1 |
| 6.3 | `SecretsRedactor` over `serde_json::Value` plus diff redaction | `core/crates/core/src/audit/redactor.rs` | 6 | 6.2 |
| 6.4 | Migration `0006_audit_immutable.sql` plus storage-side kind validation | `core/crates/adapters/migrations/0006_audit_immutable.sql`, `core/crates/adapters/src/storage_sqlite/audit.rs` | 5 | 6.2 |
| 6.5 | `AuditWriter::record` adapter wired to `Storage::record_audit_event` | `core/crates/adapters/src/audit_writer.rs` | 5 | 6.3, 6.4 |
| 6.6 | Audit query API with paginated filters | `core/crates/adapters/src/storage_sqlite/audit.rs` | 4 | 6.4 |
| 6.7 | Tracing layer that injects and propagates `correlation_id` | `core/crates/adapters/src/tracing_correlation.rs` | 4 | 6.5 |

---

## Slice 6.1 [cross-cutting] — `AuditEvent` enum and Display-to-wire mapping in `core`

### Goal

Introduce the closed `core::audit::AuditEvent` enum that names every Tier 1 audit event, and a `Display` implementation that returns the architecture §6.6 wire `kind` string verbatim. This slice ships pure-core type machinery: no I/O, no SQLite, no HTTP. It establishes the identity that every later slice writes through.

### Entry conditions

- Phase 5 (snapshot writer) is shipped; `core::snapshot::SnapshotId` exists.
- `core/crates/core/src/lib.rs` exports a `pub mod audit;` placeholder MAY exist; if not, this slice creates it.

### Files to create or modify

- `core/crates/core/src/audit/mod.rs` — module root, re-exports `event`, `row`, `redactor` (the latter two added in 6.2 and 6.3).
- `core/crates/core/src/audit/event.rs` — enum, `Display`, `FromStr`.
- `core/crates/core/src/lib.rs` — add `pub mod audit;`.

### Signatures and shapes

```rust
// core/crates/core/src/audit/event.rs

use std::fmt;
use std::str::FromStr;

/// Closed Tier 1 audit-event vocabulary. Variants map one-to-one to wire
/// `kind` strings recorded in `audit_log.kind` (architecture §6.6).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum AuditEvent {
    // Authentication (T1.14)
    AuthLoginSucceeded,
    AuthLoginFailed,
    AuthLogout,
    AuthSessionRevoked,
    AuthBootstrapCredentialsRotated,

    // Caddy lifecycle (T1.11)
    CaddyCapabilityProbeCompleted,
    CaddyOwnershipSentinelConflict,
    CaddyReconnected,
    CaddyUnreachable,

    // Configuration apply (T1.1)
    ApplySucceeded,
    ApplyFailed,
    DriftDetected,
    DriftResolved,
    ConfigRolledBack,

    // Mutation lifecycle (T1.6)
    MutationProposed,
    MutationSubmitted,
    MutationApplied,
    MutationConflicted,
    MutationRejected,
    MutationRejectedMissingExpectedVersion,

    // Secrets (T1.15)
    SecretsRevealed,
    SecretsMasterKeyRotated,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AuditEventParseError {
    #[error("audit kind {0:?} is not in the architecture §6.6 vocabulary")]
    Unknown(String),
}

impl AuditEvent {
    /// Wire form (dotted lowercase) recorded in `audit_log.kind`.
    pub const fn kind_str(&self) -> &'static str {
        match self {
            AuditEvent::AuthLoginSucceeded => "auth.login-succeeded",
            AuditEvent::AuthLoginFailed => "auth.login-failed",
            AuditEvent::AuthLogout => "auth.logout",
            AuditEvent::AuthSessionRevoked => "auth.session-revoked",
            AuditEvent::AuthBootstrapCredentialsRotated => "auth.bootstrap-credentials-rotated",
            AuditEvent::CaddyCapabilityProbeCompleted => "caddy.capability-probe-completed",
            AuditEvent::CaddyOwnershipSentinelConflict => "caddy.ownership-sentinel-conflict",
            AuditEvent::CaddyReconnected => "caddy.reconnected",
            AuditEvent::CaddyUnreachable => "caddy.unreachable",
            AuditEvent::ApplySucceeded => "config.applied",
            AuditEvent::ApplyFailed => "config.apply-failed",
            AuditEvent::DriftDetected => "config.drift-detected",
            AuditEvent::DriftResolved => "config.drift-resolved",
            AuditEvent::ConfigRolledBack => "config.rolled-back",
            AuditEvent::MutationProposed => "mutation.proposed",
            AuditEvent::MutationSubmitted => "mutation.submitted",
            AuditEvent::MutationApplied => "mutation.applied",
            AuditEvent::MutationConflicted => "mutation.conflicted",
            AuditEvent::MutationRejected => "mutation.rejected",
            AuditEvent::MutationRejectedMissingExpectedVersion =>
                "mutation.rejected.missing-expected-version",
            AuditEvent::SecretsRevealed => "secrets.revealed",
            AuditEvent::SecretsMasterKeyRotated => "secrets.master-key-rotated",
        }
    }
}

impl fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_str())
    }
}

impl FromStr for AuditEvent {
    type Err = AuditEventParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> { /* exhaustive match */ }
}

/// Compile-time regex of the §6.6 dotted form. Every `kind_str()` MUST match.
pub const AUDIT_KIND_REGEX: &str = r"^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$";
```

### Algorithm

1. Define the enum with `#[non_exhaustive]` so later phases MAY add variants without breaking external matchers.
2. The `kind_str` method is exhaustive: every variant returns a literal from the §6.6 table. Rust's exhaustiveness check catches drift.
3. `FromStr` mirrors `kind_str`. A miss returns `AuditEventParseError::Unknown(s.to_owned())`.
4. A unit test compiles `AUDIT_KIND_REGEX` once and asserts every variant's `kind_str` matches.

### Tests

- `core::audit::event::tests::display_round_trip_every_variant` — for every `AuditEvent` variant, assert `AuditEvent::from_str(v.kind_str()) == Ok(v.clone())`.
- `core::audit::event::tests::kind_strings_match_section_6_6_regex` — compile `AUDIT_KIND_REGEX` and assert each `kind_str()` matches.
- `core::audit::event::tests::unknown_kind_rejected` — `AuditEvent::from_str("not.a.kind")` returns `Err(Unknown(_))`.
- `core::audit::event::tests::tier_1_set_complete` — assert the union of `kind_str()` values equals the closed Tier 1 set listed in this slice (one assertion per variant).

### Acceptance command

`cargo test -p trilithon-core audit::event::tests`

### Exit conditions

- `core::audit::AuditEvent` compiles and exposes the Tier 1 variants listed.
- Every `kind_str()` value MUST appear in the architecture §6.6 vocabulary table.
- `Display` MUST emit the wire form, not the Rust variant name.
- `FromStr` MUST round-trip every variant.
- `cargo build -p trilithon-core` succeeds.

### Audit kinds emitted

None directly. This slice declares the vocabulary referenced in architecture §6.6. The wire forms enumerated above are the canonical strings that subsequent slices write.

### Tracing events emitted

None. This slice is pure types.

### Cross-references

- ADR-0009.
- PRD T1.7.
- Architecture §6.6 (audit `kind` vocabulary, Rust `AuditEvent` ↔ wire `kind` mapping).
- trait-signatures.md §1 `core::storage::Storage::record_audit_event` (consumer of these strings via `AuditEventRow.kind`).

---

## Slice 6.2 [standard] — `AuditEventRow`, `AuditSelector`, `AuditOutcome`, `ActorRef`

### Goal

Define the row-level record and the query selector exchanged between `core` and the storage trait. The `AuditEventRow` mirrors the `audit_log` columns from architecture §6.6, while remaining pure-core (no SQLite types). This slice produces the surface that `Storage::record_audit_event` and `Storage::tail_audit_log` consume.

### Entry conditions

- Slice 6.1 done.
- `core::snapshot::SnapshotId` is in scope (Phase 5).

### Files to create or modify

- `core/crates/core/src/audit/row.rs` — `AuditEventRow`, `AuditSelector`, `AuditOutcome`, `ActorRef`, `AuditRowId`.
- `core/crates/core/src/audit/mod.rs` — `pub mod row; pub use row::*;`.

### Signatures and shapes

```rust
use serde::{Deserialize, Serialize};
use ulid::Ulid;
use crate::audit::event::AuditEvent;
use crate::snapshot::SnapshotId;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct AuditRowId(pub Ulid);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ActorRef {
    User { id: String },
    Token { id: String },
    System { component: &'static str },
    Docker,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditOutcome { Ok, Error, Denied }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditEventRow {
    pub id:                 AuditRowId,        // ULID
    pub correlation_id:     Ulid,
    pub occurred_at:        i64,               // unix seconds (UTC)
    pub occurred_at_ms:     i64,               // millis fraction; full ms since epoch
    pub actor:              ActorRef,
    pub event:              AuditEvent,
    pub target_kind:        Option<String>,    // for example "route" or "snapshot"
    pub target_id:          Option<String>,
    pub snapshot_id:        Option<SnapshotId>,
    pub redacted_diff_json: Option<String>,    // canonical JSON, redacted
    pub redaction_sites:    u32,
    pub outcome:            AuditOutcome,
    pub error_kind:         Option<String>,    // closed enum on the wire
    pub notes:              Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditSelector {
    pub since:           Option<i64>,    // unix seconds inclusive
    pub until:           Option<i64>,    // unix seconds exclusive
    pub correlation_id:  Option<Ulid>,
    pub actor_id:        Option<String>,
    pub event:           Option<AuditEvent>,
    pub limit:           Option<u32>,    // default 100, max 1000
    pub cursor_before:   Option<AuditRowId>, // cursor pagination, descending
}

pub const AUDIT_QUERY_DEFAULT_LIMIT: u32 = 100;
pub const AUDIT_QUERY_MAX_LIMIT: u32 = 1000;
```

### Algorithm

1. `AuditEventRow` carries every column from architecture §6.6. The `actor` field projects onto `audit_log.actor_kind` plus `actor_id` at the storage boundary.
2. `AuditSelector::limit` MUST be clamped to `[1, AUDIT_QUERY_MAX_LIMIT]` at the storage boundary; values > max are clamped to max, missing values default to `AUDIT_QUERY_DEFAULT_LIMIT`. Pure-core does the validation in a `normalised()` helper that returns a struct with non-optional limits.

### Tests

- `core::audit::row::tests::serde_round_trip_full_row` — serialise and deserialise a fully populated row; assert byte-stable.
- `core::audit::row::tests::serde_round_trip_minimal_row` — only required fields; assert deserialise fills `None`s.
- `core::audit::row::tests::selector_normalises_limit` — `AuditSelector { limit: Some(9999), .. }` after `.normalised()` returns `1000`; `None` returns `100`.
- `core::audit::row::tests::actor_serialises_externally_tagged` — assert wire form is the externally-tagged enum.

### Acceptance command

`cargo test -p trilithon-core audit::row::tests`

### Exit conditions

- All three audit row types compile, derive `Serialize`/`Deserialize`, and pass the round-trip tests.
- `AUDIT_QUERY_DEFAULT_LIMIT` MUST equal 100, `AUDIT_QUERY_MAX_LIMIT` MUST equal 1000.
- `cargo build -p trilithon-core` succeeds.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Architecture §6.6 (column-by-column).
- trait-signatures.md §1 `core::storage::Storage` (`AuditEventRow` and `AuditSelector` are the wire types in `record_audit_event` and `tail_audit_log`).
- PRD T1.7.

---

## Slice 6.3 [cross-cutting] — `SecretsRedactor` over `serde_json::Value` plus diff redaction

### Goal

Ship the pure-core redactor that walks an arbitrary `serde_json::Value` tree, identifies schema-marked secret fields, and replaces their values with `"***"` plus a stable hash prefix derived from the ciphertext (or a placeholder when no ciphertext is yet known). This slice is the gate that satisfies hazard H10: no plaintext secret reaches the audit log writer.

### Entry conditions

- Slice 6.2 done.
- The Phase 4 schema registry exposes `SchemaRegistry::is_secret_field(&JsonPointer) -> bool`. If absent, this slice MUST add a minimal `SchemaRegistry` stub and a `secret_field_paths()` method backed by a static set covering the Tier 1 secret fields enumerated below.

### Files to create or modify

- `core/crates/core/src/audit/redactor.rs` — the redactor.
- `core/crates/core/src/audit/mod.rs` — `pub mod redactor;`.
- `core/crates/core/src/schema/secret_fields.rs` — the static secret-field registry (created here if Phase 4 has not already shipped it). The Tier 1 secret fields are: `/auth/basic/users/*/password`, `/forward_auth/secret`, `/headers/*/Authorization`, `/upstreams/*/auth/api_key`. New entries land in this file in subsequent phases.

### Signatures and shapes

```rust
use serde_json::Value;
use crate::schema::SchemaRegistry;

/// The redaction marker emitted in place of a plaintext secret.
pub const REDACTION_PREFIX: &str = "***";

/// Length of the truncated lowercase-hex hash prefix appended to the marker.
pub const HASH_PREFIX_LEN: usize = 12;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedactionResult {
    pub value: Value,         // the redacted tree
    pub sites: u32,           // count of secret sites replaced
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RedactorError {
    #[error("redactor would emit plaintext at {path}")]
    PlaintextDetected { path: String },
}

pub trait CiphertextHasher: Send + Sync {
    /// Stable lowercase-hex SHA-256 prefix of the ciphertext bytes for the
    /// supplied secret value. Implementations MUST return identical output
    /// for byte-identical inputs.
    fn hash_for_value(&self, plaintext: &str) -> String;
}

pub struct SecretsRedactor<'a> {
    registry: &'a SchemaRegistry,
    hasher:   &'a dyn CiphertextHasher,
}

impl<'a> SecretsRedactor<'a> {
    pub fn new(
        registry: &'a SchemaRegistry,
        hasher:   &'a dyn CiphertextHasher,
    ) -> Self;

    /// Walk a tree, replace secret-marked leaf values, return the redacted
    /// tree and the count of redacted sites.
    pub fn redact(&self, value: &Value) -> Result<RedactionResult, RedactorError>;

    /// Convenience wrapper for an entire diff representation.
    pub fn redact_diff(&self, diff: &Value) -> Result<RedactionResult, RedactorError>;
}
```

### Algorithm

1. Walk `value` recursively, tracking the current `JsonPointer` (RFC 6901) path.
2. At each leaf, ask `registry.is_secret_field(&path)`. If true and the leaf is a string, replace the leaf with `format!("{REDACTION_PREFIX}{hash}")` where `hash` is `&hasher.hash_for_value(plaintext)[..HASH_PREFIX_LEN]`. Increment `sites`.
3. If a secret-marked field carries a non-string leaf (number, boolean, array, object), the redactor MUST replace the entire subtree with `"***"` and increment `sites` exactly once.
4. After the walk, run a self-check: re-walk the redacted tree, calling `registry.is_secret_field(&path)`, and assert no leaf still resembles plaintext. The self-check is a leaf-string-prefix test: secret leaves MUST start with `REDACTION_PREFIX`. A miss returns `RedactorError::PlaintextDetected { path }`.
5. `redact_diff` accepts a diff in the `{ added, removed, modified }` shape produced by Phase 8 and applies steps 1–4 to every embedded value.

### Tests

- `core::audit::redactor::tests::redacts_basic_auth_password` — input `{"auth":{"basic":{"users":[{"password":"hunter2"}]}}}` produces `***<hash>` and `sites = 1`.
- `core::audit::redactor::tests::redacts_authorization_header` — exercises a wildcard array path.
- `core::audit::redactor::tests::redacts_object_subtree_for_secret_object` — a non-string secret leaf is replaced with `"***"`.
- `core::audit::redactor::tests::self_check_catches_plaintext_leak` — a deliberately mis-implemented hasher returning the plaintext is rejected with `PlaintextDetected`.
- `core::audit::redactor::tests::corpus_every_tier_1_secret_field` — iterate every entry in `schema::secret_fields::TIER_1_SECRET_FIELDS` and assert no plaintext byte survives.
- `core::audit::redactor::tests::deterministic_hash_prefix` — same plaintext yields same prefix across two invocations.

### Acceptance command

`cargo test -p trilithon-core audit::redactor::tests`

### Exit conditions

- For every fixture in the corpus, the redacted output contains zero bytes from the plaintext secret.
- `RedactionResult.sites` equals the corpus's expected secret count.
- The redactor MUST NOT depend on any I/O or async runtime.
- `cargo build -p trilithon-core` succeeds.

### Audit kinds emitted

None directly. The redactor produces the `redacted_diff_json` payload that 6.5 writes alongside any of the kinds enumerated in 6.1.

### Tracing events emitted

None.

### Cross-references

- ADR-0014 (master key feeds the ciphertext that the hasher fingerprints).
- PRD T1.7, T1.15.
- Hazard H10.
- Architecture §6.6 (`redacted_diff_json`, `redaction_sites`).
- trait-signatures.md §3 `core::secrets::SecretsVault::redact` (this slice's redactor MAY be wired to the vault's `redact` in Phase 10; the trait surface is compatible by design).

---

## Slice 6.4 [cross-cutting] — Migration `0006_audit_immutable.sql` plus storage-side kind validation

### Goal

Land the SQLite migration that creates the `audit_log` table per architecture §6.6 (if Phase 2 has not already done so) and adds `BEFORE UPDATE` and `BEFORE DELETE` triggers that abort with a database error. Wire insert-time validation of the `kind` string against the architecture §6.6 vocabulary in the storage adapter.

### Entry conditions

- Phase 2 (SQLite persistence) is shipped; the embedded migration runner exists in `adapters`.
- Slice 6.1 done.

### Files to create or modify

- `core/crates/adapters/migrations/0006_audit_immutable.sql` — DDL plus triggers.
- `core/crates/adapters/src/storage_sqlite/audit.rs` — insert-path validation.
- `core/crates/adapters/src/storage_sqlite/mod.rs` — register the new module.

### Signatures and shapes

```sql
-- core/crates/adapters/migrations/0006_audit_immutable.sql

-- The audit_log table (if Phase 2 did not already create it).
CREATE TABLE IF NOT EXISTS audit_log (
    id                 TEXT PRIMARY KEY,
    correlation_id     TEXT NOT NULL,
    occurred_at        INTEGER NOT NULL,
    occurred_at_ms     INTEGER NOT NULL,
    actor_kind         TEXT NOT NULL,
    actor_id           TEXT NOT NULL,
    kind               TEXT NOT NULL,
    target_kind        TEXT,
    target_id          TEXT,
    snapshot_id        TEXT REFERENCES snapshots(id),
    redacted_diff_json TEXT,
    redaction_sites    INTEGER NOT NULL DEFAULT 0,
    outcome            TEXT NOT NULL CHECK (outcome IN ('ok', 'error', 'denied')),
    error_kind         TEXT,
    notes              TEXT
);
CREATE INDEX IF NOT EXISTS audit_log_correlation_id ON audit_log(correlation_id);
CREATE INDEX IF NOT EXISTS audit_log_occurred_at    ON audit_log(occurred_at);
CREATE INDEX IF NOT EXISTS audit_log_actor_id       ON audit_log(actor_id);
CREATE INDEX IF NOT EXISTS audit_log_kind           ON audit_log(kind);

-- Immutability triggers.
CREATE TRIGGER audit_log_no_update
BEFORE UPDATE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log rows are immutable (architecture §6.6)');
END;

CREATE TRIGGER audit_log_no_delete
BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log rows are immutable (architecture §6.6)');
END;
```

```rust
// core/crates/adapters/src/storage_sqlite/audit.rs

use trilithon_core::audit::{AuditEvent, AuditEventRow, AuditRowId};
use trilithon_core::storage::StorageError;

pub(super) fn insert_audit_row(
    conn: &rusqlite::Connection,
    row:  &AuditEventRow,
) -> Result<AuditRowId, StorageError> {
    validate_kind(&row.event)?;
    /* INSERT INTO audit_log ... */
}

fn validate_kind(event: &AuditEvent) -> Result<(), StorageError> {
    // event.kind_str() is statically guaranteed in slice 6.1.
    // The check is belt-and-braces against future additions: the kind MUST
    // match AUDIT_KIND_REGEX. A miss returns AuditKindUnknown.
    Ok(())
}
```

### Algorithm

1. The migration runner detects schema version 2 and applies `0006_audit_immutable.sql`.
2. The runner records `(version=3, applied_at, description, checksum)` in `schema_migrations` per architecture §14.
3. `insert_audit_row` validates `row.event.kind_str()` against `AUDIT_KIND_REGEX` once; a miss returns `StorageError::AuditKindUnknown`.
4. Any direct `UPDATE audit_log` or `DELETE FROM audit_log` aborts with the SQLite trigger message; the adapter MUST translate the abort into `StorageError::Integrity`.

### Tests

- `core/crates/adapters/tests/audit_immutable_update_aborts.rs` — open a fresh database, insert one row, attempt `UPDATE audit_log SET notes='changed' WHERE id=?`, assert a SQLite-level error is surfaced.
- `core/crates/adapters/tests/audit_immutable_delete_aborts.rs` — attempt `DELETE FROM audit_log`, assert error.
- `core/crates/adapters/tests/audit_kind_validation.rs` — call the storage adapter with a hand-forged `AuditEventRow` whose `kind` does not match the regex (constructed via private constructor in test cfg) and assert `StorageError::AuditKindUnknown`.
- `core/crates/adapters/tests/audit_migration_idempotent.rs` — run migrations twice; the second run is a no-op.

### Acceptance command

`cargo test -p trilithon-adapters --test audit_immutable_update_aborts --test audit_immutable_delete_aborts --test audit_kind_validation --test audit_migration_idempotent`

### Exit conditions

- `audit_log` exists with the schema and indexes from architecture §6.6.
- `UPDATE audit_log` and `DELETE FROM audit_log` MUST abort at the database layer.
- A row whose `kind` is not in §6.6 MUST be rejected with `StorageError::AuditKindUnknown`.
- The migration is idempotent under repeat invocation.

### Audit kinds emitted

None directly; this slice ships the storage substrate.

### Tracing events emitted

`storage.migrations.applied` (architecture §12.1) on a successful migration run.

### Cross-references

- ADR-0009.
- PRD T1.7.
- Architecture §6.6, §14 (migrations).
- trait-signatures.md §1 `core::storage::Storage::record_audit_event` (`StorageError::AuditKindUnknown`).

---

## Slice 6.5 [cross-cutting] — `AuditWriter::record` adapter wired to `Storage::record_audit_event`

### Goal

Provide the single public surface (`AuditWriter::record(event)`) that other adapters call to persist an audit row. The writer is the only path to `audit_log`. Internally it invokes the redactor (slice 6.3) on any diff payload before delegating to `Storage::record_audit_event` (the trait method landed by Phase 2 and validated in slice 6.4).

### Entry conditions

- Slices 6.3 and 6.4 done.
- The `Storage` trait object is available to adapter callers via `Arc<dyn Storage>` from Phase 2.

### Files to create or modify

- `core/crates/adapters/src/audit_writer.rs` — the writer.
- `core/crates/adapters/src/lib.rs` — add `pub mod audit_writer;` and re-export `AuditWriter`.

### Signatures and shapes

```rust
use std::sync::Arc;
use trilithon_core::audit::{AuditEvent, AuditEventRow, AuditOutcome, ActorRef, AuditRowId};
use trilithon_core::audit::redactor::SecretsRedactor;
use trilithon_core::storage::{Storage, StorageError};
use ulid::Ulid;

#[derive(Clone, Debug, thiserror::Error)]
pub enum AuditWriteError {
    #[error("redaction failed: {0}")]
    Redaction(#[from] trilithon_core::audit::redactor::RedactorError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct AuditWriter {
    storage:  Arc<dyn Storage>,
    redactor: Arc<dyn Fn(&serde_json::Value) -> Result<serde_json::Value, AuditWriteError> + Send + Sync>,
    clock:    Arc<dyn trilithon_core::clock::Clock>,
}

#[derive(Clone, Debug)]
pub struct AuditAppend {
    pub correlation_id: Ulid,
    pub actor:          ActorRef,
    pub event:          AuditEvent,
    pub target_kind:    Option<String>,
    pub target_id:      Option<String>,
    pub snapshot_id:    Option<trilithon_core::snapshot::SnapshotId>,
    pub diff:           Option<serde_json::Value>,  // un-redacted; writer redacts
    pub outcome:        AuditOutcome,
    pub error_kind:     Option<String>,
    pub notes:          Option<String>,
}

impl AuditWriter {
    pub fn new(
        storage:  Arc<dyn Storage>,
        clock:    Arc<dyn trilithon_core::clock::Clock>,
        redactor: SecretsRedactor<'static>,
    ) -> Self;

    /// The single, public path into `audit_log`. Returns the persisted row id.
    pub async fn record(&self, append: AuditAppend) -> Result<AuditRowId, AuditWriteError>;
}
```

### Algorithm

1. Generate a fresh `AuditRowId(Ulid::new())`.
2. Read `now = clock.now_unix_ms()`; derive `occurred_at` (seconds) and `occurred_at_ms` (full ms).
3. If `append.diff` is `Some`, run the redactor; capture `(redacted_value, sites)`. Serialise `redacted_value` to canonical JSON. Otherwise `redacted_diff_json = None, sites = 0`.
4. Construct an `AuditEventRow` from the inputs.
5. Delegate to `self.storage.record_audit_event(row).await`.
6. On `Ok(id)` return `id`. On `Err(StorageError)` return `AuditWriteError::Storage(_)`. The redactor's `RedactorError` short-circuits before the storage call.

### Tests

- `core/crates/adapters/tests/audit_writer_happy_path.rs` — record a `MutationApplied` event with a diff containing a secret; assert the row exists, `redacted_diff_json` contains `***`, `redaction_sites > 0`, and the plaintext substring is absent.
- `core/crates/adapters/tests/audit_writer_no_diff.rs` — record `AuthLoginSucceeded` with `diff = None`; assert `redacted_diff_json IS NULL` and `redaction_sites = 0`.
- `core/crates/adapters/tests/audit_writer_storage_failure_propagates.rs` — inject a `Storage` double that returns `StorageError::SqliteBusy`; assert `AuditWriteError::Storage(_)` surfaces.
- `core/crates/adapters/tests/audit_writer_no_bypass.rs` — `git grep` style compile-fail test asserting no other call site reaches `Storage::record_audit_event` directly. Implementation: a `cargo deny` style allow-list, or a `#[cfg(test)]` symbol-presence assertion in a doc test that fails if `record_audit_event` is referenced outside `audit_writer.rs`.

### Acceptance command

`cargo test -p trilithon-adapters audit_writer`

### Exit conditions

- Every successful `AuditWriter::record` invocation MUST result in exactly one `audit_log` row.
- The redactor MUST run on every non-empty `diff`; bypass MUST be impossible at the adapter boundary.
- `cargo build -p trilithon-adapters` succeeds.

### Audit kinds emitted

Any kind from architecture §6.6 may flow through this writer. The Tier 1 set listed in slice 6.1 is the closed set this phase contemplates.

### Tracing events emitted

None directly. Tracing instrumentation lands in slice 6.7.

### Cross-references

- ADR-0009.
- PRD T1.7, T1.15.
- Architecture §6.6, §7.1 step 5 and step 10 (audit append points).
- trait-signatures.md §1 `Storage::record_audit_event`, §3 `SecretsVault::redact`.

---

## Slice 6.6 [standard] — Audit query API with paginated filters

### Goal

Implement `Storage::tail_audit_log` for the SQLite adapter, exposing time-range, correlation-id, actor, and event-kind filters with cursor-based pagination at default 100 / max 1000 rows. Phase 9 surfaces this via HTTP; this slice ships the storage-side mechanics.

### Entry conditions

- Slice 6.4 done; `audit_log` exists.
- Slice 6.5 done; rows can be written.

### Files to create or modify

- `core/crates/adapters/src/storage_sqlite/audit.rs` — extend with `tail_audit_log` implementation.
- `core/crates/adapters/src/storage_sqlite/mod.rs` — wire the trait method.

### Signatures and shapes

```rust
// Implements core::storage::Storage::tail_audit_log for SqliteStorage.
async fn tail_audit_log(
    &self,
    selector: AuditSelector,
    limit:    u32,
) -> Result<Vec<AuditEventRow>, StorageError>;
```

The SQL emitted for a fully-populated selector:

```sql
SELECT id, correlation_id, occurred_at, occurred_at_ms,
       actor_kind, actor_id, kind, target_kind, target_id,
       snapshot_id, redacted_diff_json, redaction_sites,
       outcome, error_kind, notes
FROM audit_log
WHERE (:since IS NULL OR occurred_at >= :since)
  AND (:until IS NULL OR occurred_at <  :until)
  AND (:correlation_id IS NULL OR correlation_id = :correlation_id)
  AND (:actor_id IS NULL OR actor_id = :actor_id)
  AND (:kind IS NULL OR kind = :kind)
  AND (:cursor_before IS NULL OR id < :cursor_before)
ORDER BY id DESC
LIMIT :limit;
```

### Algorithm

1. Normalise `selector.limit`: `min(selector.limit.unwrap_or(100), 1000).max(1)`. The caller-supplied `limit` parameter on the trait signature is the authoritative cap.
2. Bind parameters; missing filters bind as `NULL`. Run the prepared statement.
3. Map each row through a `from_row` helper that reconstructs `AuditEventRow` (decoding `actor_kind/actor_id` into `ActorRef`, parsing `kind` via `AuditEvent::from_str`).
4. If a row's `kind` does not parse, return `StorageError::Integrity { detail: "unknown audit kind in row …" }` rather than silently discarding the row. The §6.6 vocabulary is closed; an unknown kind in the table indicates corruption.

### Tests

- `core/crates/adapters/tests/audit_query_pagination.rs` — insert 250 rows; assert default limit returns 100, explicit `limit=1000` returns 250, `limit=5000` is clamped to 1000.
- `core/crates/adapters/tests/audit_query_correlation_filter.rs` — three rows share one correlation id; the filter returns exactly those rows.
- `core/crates/adapters/tests/audit_query_time_range.rs` — `since`/`until` half-open interval is honoured; boundary row at `until` is excluded.
- `core/crates/adapters/tests/audit_query_event_filter.rs` — only rows with `event = AuditEvent::ApplySucceeded` return.
- `core/crates/adapters/tests/audit_query_actor_filter.rs` — actor scoping returns only matching rows.
- `core/crates/adapters/tests/audit_query_cursor_descending.rs` — paginate through 100-row batches using `cursor_before` and assert no row is returned twice and no row is skipped.

### Acceptance command

`cargo test -p trilithon-adapters audit_query`

### Exit conditions

- Every selector field is honoured.
- Pagination MUST be stable: a stable insertion order plus `cursor_before` produces a repeatable, exhaustive walk.
- Default page size MUST be 100; max MUST be 1000.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.7.
- Architecture §6.6.
- trait-signatures.md §1 `Storage::tail_audit_log`.

---

## Slice 6.7 [cross-cutting] — Tracing layer that injects and propagates `correlation_id`

### Goal

Land the `tracing` middleware that ensures every audit-emitting code path runs inside a span carrying a `correlation_id` field. The layer reads the `X-Correlation-Id` HTTP header if present, otherwise generates a fresh ULID; it also wraps background-task entry points (drift loop, signal handlers). Per architecture cross-trait invariants, no implementor generates a fresh `correlation_id` on entry; the span is the source.

### Entry conditions

- Slice 6.5 done; `AuditWriter` is the single audit path.
- Phase 1 (daemon skeleton) shipped a `tracing-subscriber` initialisation site.

### Files to create or modify

- `core/crates/adapters/src/tracing_correlation.rs` — the layer plus a `with_correlation_span` helper.
- `core/crates/adapters/src/lib.rs` — re-export.
- `core/crates/cli/src/main.rs` — register the layer at subscriber init.

### Signatures and shapes

```rust
use tracing::Span;
use ulid::Ulid;

/// The canonical span field key (architecture §12.1).
pub const CORRELATION_ID_FIELD: &str = "correlation_id";

/// Read the correlation id from the current span, falling back to a generated
/// ULID if the field is absent. Background tasks MUST call this exactly once
/// per iteration to seed the iteration's span.
pub fn current_correlation_id() -> Ulid;

/// Wrap an async future in a span that carries `correlation_id` and any
/// supplied actor fields. Used by HTTP middleware and background tasks.
pub fn with_correlation_span<F: std::future::Future>(
    correlation_id: Ulid,
    actor_kind:     &'static str,
    actor_id:       &str,
    fut:            F,
) -> impl std::future::Future<Output = F::Output>;

/// Tower / axum middleware that reads `X-Correlation-Id`, generates one if
/// absent, and stamps the inbound request's span with both `correlation_id`
/// and `http.method`, `http.path`. Phase 9 attaches it.
pub fn correlation_layer() -> tower::layer::util::Identity; // concrete type
```

### Algorithm

1. The HTTP layer (registered in Phase 9; this slice ships the constructor) reads `request.headers().get("X-Correlation-Id")`. On parse failure or absence, generate `Ulid::new()`.
2. Open a span with `tracing::info_span!("http.request.received", correlation_id = %id, http.method = %method, http.path = %path)` per architecture §12.1.
3. `current_correlation_id()` reads the value off the current span via `tracing::span::current().field("correlation_id")`. A missing field is an architectural bug: the function MUST return a freshly generated ULID and emit a `warn!` event named `correlation_id.missing` (this event name SHOULD be added to architecture §12.1 in the same commit).
4. Background loops (drift detector, capability probe scheduler) call `with_correlation_span(Ulid::new(), "system", component_name, fut)` once per iteration.
5. The audit writer (slice 6.5) MUST call `current_correlation_id()` if the caller did not supply one explicitly. The default is to fail-loud rather than silently invent.

### Tests

- `core/crates/adapters/tests/tracing_propagates_header_correlation_id.rs` — fire an `axum` request with `X-Correlation-Id: 01ARZ3NDEKTSV4RRFFQ69G5FAV`; capture the inbound span; assert the field equals the header value.
- `core/crates/adapters/tests/tracing_generates_correlation_id_on_absence.rs` — no header; assert a valid ULID is generated.
- `core/crates/adapters/tests/audit_row_has_non_null_correlation_id.rs` — perform N=50 random audit writes through the writer under a span; SELECT all rows; assert no row has a null `correlation_id`.
- `core/crates/adapters/tests/background_task_seeds_per_iteration.rs` — run a fake drift loop for three iterations; assert three distinct correlation ids appear.

### Acceptance command

`cargo test -p trilithon-adapters tracing_ audit_row_has_non_null_correlation_id background_task_seeds`

### Exit conditions

- Every audit row written through `AuditWriter::record` MUST carry a non-null `correlation_id`.
- The HTTP layer MUST honour `X-Correlation-Id` when present and generate a ULID otherwise.
- Background tasks MUST seed a new correlation id per iteration.

### Audit kinds emitted

This slice does not emit any audit row directly; it instruments callers that do.

### Tracing events emitted

`http.request.received`, `http.request.completed` (architecture §12.1). The `correlation_id.missing` event is flagged as an open question below.

### Cross-references

- PRD T1.7.
- Architecture §12 (correlation-id propagation rule), §12.1 (span field key `correlation_id`).
- trait-signatures.md "Cross-trait invariants → Correlation propagation".

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] `cargo test -p trilithon-core audit::` and `cargo test -p trilithon-adapters audit_*` pass.
- [ ] No code path writes to `audit_log` without going through `AuditWriter::record` (slice 6.5 enforcement test).
- [ ] Every diff written to `redacted_diff_json` passes the redactor; the corpus covers every Tier 1 secret-marked field (slice 6.3).
- [ ] Every audit row carries a non-null `correlation_id` (slice 6.7 invariant test).
- [ ] Any attempt to `UPDATE` or `DELETE` an `audit_log` row fails at the database layer (slice 6.4 triggers).
- [ ] `core/README.md` records the audit pipeline and the redactor invariant, citing ADR-0009.

## Open questions

- The `correlation_id.missing` warn-level event in slice 6.7 is not yet listed in architecture §12.1. The slice flags adding it as part of the same commit; the prompt forbids inventing names silently.
- Slice 6.3's `CiphertextHasher` requires a stable hash for plaintext that has not yet been encrypted (for example, when an audit row is written before the secrets vault from Phase 10 lands). The current draft uses a per-process random salt; whether the salt should be derived from the master key once the vault exists is a Phase 10 follow-up, not a Phase 6 deliverable.

# Phase 02 — SQLite persistence and migration framework — Implementation Slices

> Phase reference: [../phases/phase-02-sqlite-persistence.md](../phases/phase-02-sqlite-persistence.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md) §phase-2--sqlite-persistence-and-migration-framework
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference: [`../phases/phase-02-sqlite-persistence.md`](../phases/phase-02-sqlite-persistence.md).
- Architecture §6 (data model — every table), §6.1 through §6.13, §6.6 (audit `kind` vocabulary), §10 (failure model H14), §12.1 (`storage.migrations.applied`, `storage.integrity-check.failed`).
- Trait signatures: `core::storage::Storage` (§1) — implemented end-to-end in this phase.
- ADR-0006 (SQLite), ADR-0009 (immutable snapshots and audit log), ADR-0012 (optimistic concurrency).

## Slice plan summary

| Slice | Title | Primary files | Effort (h) | Depends on |
|-------|-------|---------------|------------|------------|
| 2.1 | `Storage` trait surface and `StorageError` | `crates/core/src/storage/{mod,error}.rs` | 5 | Phase 1 |
| 2.2 | `InMemoryStorage` test double | `crates/core/src/storage/in_memory.rs` | 4 | 2.1 |
| 2.3 | `0001_init.sql` migration with seven Tier 1 tables | `crates/adapters/migrations/0001_init.sql` | 5 | 2.1 |
| 2.4 | `SqliteStorage` adapter, pragmas, advisory lock | `crates/adapters/src/sqlite_storage.rs` | 6 | 2.3 |
| 2.5 | Migration runner with downgrade refusal | `crates/adapters/src/migrate.rs` | 4 | 2.4 |
| 2.6 | Periodic `PRAGMA integrity_check` task | `crates/adapters/src/integrity_check.rs` | 3 | 2.4 |
| 2.7 | Wire startup, exit code 3 on storage failure, integration tests | `crates/cli/src/main.rs`, `crates/adapters/tests/sqlite_storage.rs` | 5 | 2.4–2.6 |

Total: 7 slices.

---

## Slice 2.1 [cross-cutting] — `Storage` trait surface and `StorageError`

### Goal

Define the `Storage` trait and `StorageError` enum verbatim from trait-signatures.md §1, plus the supporting row types (`Snapshot`, `AuditEventRow`, `DriftEventRow`, `ProposalRow`, `AuditSelector`, `ParentChain`, `SnapshotId`, `AuditRowId`, `ProposalId`, `DriftRowId`, `UnixSeconds`) inside `core`. The trait is async, object-safe, and free of SQLite types.

### Entry conditions

- Phase 1 complete; `trilithon-core` builds.
- `crates/core/Cargo.toml` adds `serde = { version = "1", features = ["derive"] }`, `serde_json = "1.0"`, `async-trait = "0.1"`, `sha2 = "0.10"`, `ulid = { version = "1", features = ["serde"] }`. No `sqlx` or `tokio`. (`serde` is required for row type derive macros; `serde_json` is required for the `canonical_json_for_hash()` helper.)

### Files to create or modify

- `core/crates/core/src/storage/mod.rs` — re-exports (new).
- `core/crates/core/src/storage/types.rs` — row and value types (new).
- `core/crates/core/src/storage/error.rs` — `StorageError` (new).
- `core/crates/core/src/storage/trait_def.rs` — the `Storage` trait (new).
- `core/crates/core/src/storage/helpers.rs` — `canonical_json_for_hash()` helper (new).
- `core/crates/core/src/lib.rs` — `pub mod storage;` (modify).

### Signatures and shapes

```rust
// core/crates/core/src/storage/types.rs
use serde::{Deserialize, Serialize};

pub type UnixSeconds = i64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub String);          // sha256 hex, 64 chars

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuditRowId(pub String);          // ULID

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProposalId(pub String);          // ULID

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DriftRowId(pub String);          // ULID

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id:                       SnapshotId,
    pub parent_id:                Option<SnapshotId>,
    pub caddy_instance_id:        String,                 // V1: always "local"
    pub actor_kind:               ActorKind,
    pub actor_id:                 String,
    pub intent:                   String,                 // length-bounded at 4 KiB
    pub correlation_id:           String,                 // ULID
    pub caddy_version:            String,
    pub trilithon_version:        String,
    pub created_at:               UnixSeconds,
    pub created_at_ms:            i64,
    pub created_at_monotonic_ns:  i64,                    // tiebreaker for ordering within a single daemon run; architecture §6.5
    pub config_version:           i64,
    pub desired_state_json:       String,                 // canonical JSON
    pub canonical_json_version:   i64,                    // tracks canonical JSON format version; architecture §6.5
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind { User, Token, System }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventRow {
    pub id:                 AuditRowId,
    pub correlation_id:     String,
    pub occurred_at:        UnixSeconds,
    pub occurred_at_ms:     i64,
    pub actor_kind:         ActorKind,
    pub actor_id:           String,
    pub kind:               String,         // §6.6 vocabulary
    pub target_kind:        Option<String>,
    pub target_id:          Option<String>,
    pub snapshot_id:        Option<SnapshotId>,
    pub prev_hash:          String,         // SHA-256 of previous row's canonical JSON (or all-zero for first row); ADR-0009
    pub redacted_diff_json: Option<String>,
    pub redaction_sites:    u32,
    pub outcome:            AuditOutcome,
    pub error_kind:         Option<String>,
    pub notes:              Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditOutcome { Ok, Error, Denied }

#[derive(Debug, Clone, Default)]
pub struct AuditSelector {
    pub kind_glob:       Option<String>,
    pub actor_id:        Option<String>,
    pub correlation_id:  Option<String>,
    pub since:           Option<UnixSeconds>,
    pub until:           Option<UnixSeconds>,
}

#[derive(Debug, Clone)]
pub struct ParentChain {
    pub snapshots: Vec<Snapshot>,            // oldest first
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftEventRow {
    pub id:                 DriftRowId,
    pub correlation_id:     String,
    pub detected_at:        UnixSeconds,
    pub snapshot_id:        SnapshotId,
    pub diff_json:          String,
    pub resolution:         Option<DriftResolution>,
    pub resolved_at:        Option<UnixSeconds>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftResolution { Reapplied, Accepted, RolledBack }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalRow {
    pub id:               ProposalId,
    pub correlation_id:   String,
    pub source:           ProposalSource,
    pub source_ref:       Option<String>,
    pub payload_json:     String,
    pub rationale:        Option<String>,
    pub submitted_at:     UnixSeconds,
    pub expires_at:       UnixSeconds,
    pub state:            ProposalState,
    pub wildcard_callout: bool,
    // Approval-side fields (architecture §6.8; populated in later phases)
    pub decided_by_kind:  Option<String>,
    pub decided_by_id:    Option<String>,
    pub decided_at:       Option<UnixSeconds>,
    pub wildcard_ack_by:  Option<String>,
    pub wildcard_ack_at:  Option<UnixSeconds>,
    pub resulting_mutation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalSource { Docker, Llm, Import }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalState { Pending, Approved, Rejected, Expired, Superseded }
```

```rust
// core/crates/core/src/storage/error.rs (matches trait-signatures.md §1 verbatim)
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("integrity check failed: {detail}")]
    Integrity { detail: String },
    #[error("audit kind {kind} is not in the §6.6 vocabulary")]
    AuditKindUnknown { kind: String },
    #[error("snapshot {id:?} already exists")]
    SnapshotDuplicate { id: SnapshotId },
    #[error("proposal duplicate for ({source}, {source_ref})")]
    ProposalDuplicate { source: String, source_ref: String },
    #[error("sqlite busy (exceeded busy_timeout); application-level retry logic may be added in mutation queue layer")]
    SqliteBusy,
    #[error("sqlite error: {kind:?}")]
    Sqlite { kind: SqliteErrorKind },
    #[error("schema migration {version} failed: {detail}")]
    Migration { version: u32, detail: String },
    #[error("io error: {source}")]
    Io { #[source] source: std::io::Error },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqliteErrorKind { Constraint, Locked, Corrupt, Other(String) }
```

```rust
// core/crates/core/src/storage/trait_def.rs
use async_trait::async_trait;
use crate::storage::{types::*, error::StorageError};

#[async_trait]
pub trait Storage: Send + Sync + 'static {
    async fn insert_snapshot(&self, snapshot: Snapshot) -> Result<SnapshotId, StorageError>;
    async fn get_snapshot(&self, id: &SnapshotId) -> Result<Option<Snapshot>, StorageError>;
    async fn parent_chain(&self, leaf: &SnapshotId, max_depth: usize) -> Result<ParentChain, StorageError>;
    async fn latest_desired_state(&self) -> Result<Option<Snapshot>, StorageError>;
    async fn record_audit_event(&self, event: AuditEventRow) -> Result<AuditRowId, StorageError>;
    async fn tail_audit_log(&self, selector: AuditSelector, limit: u32) -> Result<Vec<AuditEventRow>, StorageError>;
    async fn record_drift_event(&self, event: DriftEventRow) -> Result<DriftRowId, StorageError>;
    async fn latest_drift_event(&self) -> Result<Option<DriftEventRow>, StorageError>;
    async fn enqueue_proposal(&self, proposal: ProposalRow) -> Result<ProposalId, StorageError>;
    async fn dequeue_proposal(&self) -> Result<Option<ProposalRow>, StorageError>;
    async fn expire_proposals(&self, now: UnixSeconds) -> Result<u32, StorageError>;
}
```

### Algorithm

Trait definition only; no algorithm. Manifest must NOT introduce `tokio` or `sqlx` to `core` (architecture §5).

Also define a reusable helper in `core/crates/core/src/storage/helpers.rs`:

```rust
use serde_json::{json, Value};
use std::collections::BTreeMap;
use crate::storage::types::AuditEventRow;

/// Serialise an AuditEventRow to canonical JSON for hashing (ADR-0009).
/// Returns a JSON object with all fields except `prev_hash`, using lexicographically sorted keys.
/// This ensures the same row produces the same hash across all implementations.
pub fn canonical_json_for_hash(row: &AuditEventRow) -> String {
    // Build a BTreeMap (lexicographically sorted) with all AuditEventRow fields except prev_hash.
    // This guarantees deterministic JSON output regardless of struct field declaration order.
    let mut map = BTreeMap::new();
    map.insert("id", json!(row.id.0));
    map.insert("correlation_id", json!(row.correlation_id));
    map.insert("occurred_at", json!(row.occurred_at));
    map.insert("occurred_at_ms", json!(row.occurred_at_ms));
    map.insert("actor_kind", json!(row.actor_kind));
    map.insert("actor_id", json!(row.actor_id));
    map.insert("kind", json!(row.kind));
    map.insert("target_kind", json!(row.target_kind));
    map.insert("target_id", json!(row.target_id));
    map.insert("snapshot_id", json!(row.snapshot_id.as_ref().map(|id| &id.0)));
    map.insert("redacted_diff_json", json!(row.redacted_diff_json));
    map.insert("redaction_sites", json!(row.redaction_sites));
    map.insert("outcome", json!(row.outcome));
    map.insert("error_kind", json!(row.error_kind));
    map.insert("notes", json!(row.notes));
    // Do NOT include prev_hash in the canonical form.
    serde_json::to_string(&map).expect("BTreeMap serialization cannot fail")
}
```

```rust
// core/crates/core/src/storage/mod.rs
pub mod audit_vocab;
pub mod error;
pub mod helpers;
pub mod types;
pub mod trait_def;

pub use error::StorageError;
pub use trait_def::Storage;
pub use types::*;

#[cfg(test)]
pub mod in_memory;
```

Both `InMemoryStorage` and `SqliteStorage` call this helper when computing `prev_hash` values. The helper guarantees lexicographic key order using `BTreeMap`, ensuring different implementations produce identical hashes for the same row. The `helpers` module is publicly exported so adapters can import `canonical_json_for_hash`.

### Tests

- `core/crates/core/src/storage/trait_def.rs` `mod tests::trait_is_pure` — compile-only test that asserts `Storage: Send + Sync` and that `dyn Storage` is constructable (object-safety check) via `let _ : Box<dyn Storage> = panic!();` inside an `#[allow(unreachable_code)]` `fn _check()`.
- `core/crates/core/src/storage/error.rs` `mod tests::display_round_trip` — instantiates each `StorageError` variant and asserts `Display` output is non-empty.

### Acceptance command

```
cargo test -p trilithon-core storage::
```

### Exit conditions

- `Storage` trait compiles, is object-safe, has no SQLite or `tokio` types.
- `StorageError` matches trait-signatures.md §1 verbatim.
- Two named tests pass.

### Audit kinds emitted

None at this slice; the trait's `record_audit_event` writes them later phases.

### Tracing events emitted

None.

### Cross-references

- Trait signatures §1.
- Architecture §6 (data model row shapes).
- ADR-0006.

---

## Slice 2.2 [cross-cutting] — `InMemoryStorage` test double

### Goal

Provide a `#[cfg(test)]`-gated `InMemoryStorage` that satisfies the `Storage` trait. The double uses `tokio::sync::RwLock<HashMap<...>>`-style structures inside `cfg(test)` only to keep `core` free of `tokio` in production builds. Per architecture §5 the double lives inside `core` only behind `#[cfg(test)]`; production callers in `cli` use `SqliteStorage` from `adapters`.

Per cross-trait invariants in trait-signatures.md, every trait now ships with a paired test double; the broader convention places doubles under `crates/adapters/tests/doubles/`. For Phase 2, the in-memory `Storage` double is the canonical contract test target and lives at the path stated in the phase reference.

### Entry conditions

- Slice 2.1 complete.
- `crates/core/Cargo.toml` declares `tokio = { version = "1", features = ["sync", "macros", "rt"], optional = true }` under a `dev-dependencies` block, NOT a regular dependency.

### Files to create or modify

- `core/crates/core/src/storage/in_memory.rs` — `#[cfg(test)] pub struct InMemoryStorage` (new).
- `core/crates/core/src/storage/audit_vocab.rs` — `pub const AUDIT_KIND_VOCAB` list (new).
- `core/crates/core/src/storage/mod.rs` — `pub mod audit_vocab; #[cfg(test)] pub mod in_memory;` (modify).

### Signatures and shapes

```rust
// core/crates/core/src/storage/in_memory.rs
#![cfg(test)]
use crate::storage::{trait_def::Storage, types::*, error::*};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::Mutex;  // NOT std::sync::Mutex — tokio::sync avoids poisoning in async contexts

pub struct InMemoryStorage {
    snapshots:    Mutex<HashMap<SnapshotId, Snapshot>>,
    audit:        Mutex<Vec<AuditEventRow>>,
    drift:        Mutex<Vec<DriftEventRow>>,
    proposals:    Mutex<Vec<ProposalRow>>,
    latest_ptr:   Mutex<Option<SnapshotId>>,
}

impl InMemoryStorage {
    pub fn new() -> Self { /* ... */ }
}

#[async_trait]
impl Storage for InMemoryStorage { /* every method */ }
```

The double MUST validate every audit `kind` against the §6.6 vocabulary; failure returns `StorageError::AuditKindUnknown`. The vocabulary list lives once in `core/crates/core/src/storage/audit_vocab.rs` as a `const &'static [&'static str]`. The Phase 6 audit writer will add new audit event types — `audit_vocab.rs` MUST be updated in lock-step. Add a unit test in Phase 2 that asserts all known Phase 2 audit kinds (`"config.applied"`, `"caddy.capability-probe-completed"`, `"mutation.rejected.missing-expected-version"`) are in `AUDIT_KIND_VOCAB`. In Phase 6, define `AuditEvent::all_kinds() -> &'static [&'static str]` that returns all variants' string names, and add a Phase 6 CI test: `assert_eq!(AuditEvent::all_kinds(), AUDIT_KIND_VOCAB)`.

### Algorithm

1. `insert_snapshot` — locks `snapshots`, rejects duplicates with `SnapshotDuplicate`, otherwise inserts and updates `latest_ptr` if the new `config_version` exceeds the current latest.
2. `record_audit_event` — validates `event.kind` against the vocabulary; computes `prev_hash` by finding the predecessor row (using the same ordering as `SqliteStorage`: the row with the largest `id` if multiple rows share the same `occurred_at`; this is equivalent to insertion order when rows are stored in ULID-sorted order), serializing it to canonical JSON using the shared `canonical_json_for_hash()` helper, computing `SHA256`, and setting `event.prev_hash`; if no prior row, sets `prev_hash` to all zeros; then pushes the event onto `audit`. MUST maintain the `audit` vec sorted by `(occurred_at ASC, id ASC)` so that `.last()` always returns the insertion-order predecessor. The canonical JSON helper is defined in `core::storage` and used by both adapters. Ordering invariant: `audit` is maintained in insertion order (ascending by ULID timestamp, ascending by ULID bits within a second), so the predecessor is always `audit.last()` before push.
3. `tail_audit_log` — applies the selector predicates, returns up to `limit` rows in reverse chronological order.
4. `enqueue_proposal` — rejects duplicate `(source, source_ref)` among rows where `state == Pending`.
5. `dequeue_proposal` — atomically locks `proposals`, finds the oldest pending proposal by `min_by_key(|p| p.submitted_at)`, removes it from the vec, and returns it (destructive dequeue). Returns `None` if no pending proposal exists. This matches the SQL adapter's destructive DELETE behavior.
6. `expire_proposals` — flips state to `Expired` for rows where `expires_at <= now`.

### Tests

- `core/crates/core/src/storage/in_memory.rs::tests::contract::insert_then_get_snapshot_round_trip`.
- `tests::contract::duplicate_snapshot_rejected`.
- `tests::contract::audit_kind_unknown_rejected` — passes a `kind` not in §6.6 (`"made.up"`), asserts `AuditKindUnknown`.
- `tests::contract::audit_kind_known_accepted` — passes `"config.applied"`, asserts `Ok`.
- `tests::contract::audit_vocabulary_complete` — asserts that all Phase 2 known kinds are present in `AUDIT_KIND_VOCAB`.
- `tests::contract::hash_chain_prev_hash_links_rows` — inserts two audit events, asserts the second event's `prev_hash` equals `SHA256(canonical_json(first_event))` and is not all-zero.
- `tests::contract::tail_audit_log_filters_correctly`.
- `tests::contract::proposal_dedup_on_source_pair`.
- `tests::contract::dequeue_proposal_removes_and_returns` — inserts a pending proposal, calls `dequeue_proposal`, asserts it returns the proposal and is no longer in the list.
- `tests::contract::expire_proposals_counts`.

### Acceptance command

```
cargo test -p trilithon-core storage::in_memory::tests::contract
```

### Exit conditions

- All seven contract tests pass.
- The double exists only behind `#[cfg(test)]`; release builds do not pull `async-trait` machinery into a non-test path beyond what the trait already requires.
- The §6.6 vocabulary list lives in exactly one file (`audit_vocab.rs`).

### Audit kinds emitted

The double does not emit; tests pass `"config.applied"`, `"caddy.capability-probe-completed"`, and `"mutation.rejected.missing-expected-version"` from §6.6 to exercise the validator.

### Tracing events emitted

None.

### Cross-references

- Trait signatures §1, "Test doubles" cross-trait invariant.
- Architecture §6.6.

---

## Slice 2.3 [cross-cutting] — `0001_init.sql` migration with seven Tier 1 tables

### Goal

Author the initial migration. It MUST create the following eight tables: `caddy_instances`, `users`, `sessions`, `snapshots`, `audit_log`, `mutations`, `proposals`, `secrets_metadata`. Every table that holds row-level data MUST carry `caddy_instance_id TEXT NOT NULL DEFAULT 'local'`. The `_sqlx_migrations` table is created by sqlx at runtime and MUST NOT be in the DDL.

### Entry conditions

- Slice 2.1 and 2.2 complete (so trait definitions match the schema).

### Files to create or modify

- `core/crates/adapters/migrations/0001_init.sql` — DDL (new).
- `core/crates/adapters/migrations/README.md` — up-only policy (new).

### Signatures and shapes

```sql
-- core/crates/adapters/migrations/0001_init.sql
-- Note: PRAGMA application_id is set programmatically after migrations complete (slice 2.7)
PRAGMA foreign_keys = ON;

CREATE TABLE caddy_instances (
    id              TEXT PRIMARY KEY,
    display_name    TEXT NOT NULL,
    transport       TEXT NOT NULL CHECK (transport IN ('unix', 'loopback_mtls')),
    address         TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    last_seen_at    INTEGER,
    capability_json TEXT,
    ownership_token TEXT NOT NULL
);

CREATE TABLE users (
    id               TEXT PRIMARY KEY,
    caddy_instance_id TEXT NOT NULL DEFAULT 'local',
    username         TEXT NOT NULL UNIQUE,
    password_hash    TEXT NOT NULL,
    role             TEXT NOT NULL CHECK (role IN ('owner', 'operator', 'reader')),
    created_at       INTEGER NOT NULL,
    must_change_pw   INTEGER NOT NULL DEFAULT 0,
    disabled_at      INTEGER
);
CREATE INDEX users_disabled_at ON users(disabled_at);

CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,
    caddy_instance_id TEXT NOT NULL DEFAULT 'local',
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL,
    expires_at      INTEGER NOT NULL,
    revoked_at      INTEGER,
    user_agent      TEXT,
    ip_address      TEXT
);
CREATE INDEX sessions_user_id ON sessions(user_id);
CREATE INDEX sessions_expires_at ON sessions(expires_at);

CREATE TABLE snapshots (
    id                       TEXT PRIMARY KEY,
    parent_id                TEXT REFERENCES snapshots(id) CHECK (parent_id != id),
    caddy_instance_id        TEXT NOT NULL DEFAULT 'local',
    actor_kind               TEXT NOT NULL CHECK (actor_kind IN ('user', 'token', 'system')),
    actor_id                 TEXT NOT NULL,
    intent                   TEXT NOT NULL,
    correlation_id           TEXT NOT NULL,
    caddy_version            TEXT NOT NULL,
    trilithon_version        TEXT NOT NULL,
    created_at               INTEGER NOT NULL,
    created_at_ms            INTEGER NOT NULL,
    created_at_monotonic_ns  INTEGER NOT NULL,
    config_version           INTEGER NOT NULL,
    canonical_json_version   INTEGER NOT NULL DEFAULT 1,
    desired_state_json       TEXT NOT NULL
);
CREATE INDEX snapshots_parent_id ON snapshots(parent_id);
CREATE INDEX snapshots_correlation_id ON snapshots(correlation_id);
CREATE INDEX snapshots_caddy_instance_id ON snapshots(caddy_instance_id);
CREATE UNIQUE INDEX snapshots_config_version ON snapshots(caddy_instance_id, config_version);

CREATE TABLE audit_log (
    id                 TEXT PRIMARY KEY,
    caddy_instance_id  TEXT NOT NULL DEFAULT 'local',
    correlation_id     TEXT NOT NULL,
    occurred_at        INTEGER NOT NULL,
    occurred_at_ms     INTEGER NOT NULL,
    actor_kind         TEXT NOT NULL,
    actor_id           TEXT NOT NULL,
    kind               TEXT NOT NULL,
    target_kind        TEXT,
    target_id          TEXT,
    snapshot_id        TEXT REFERENCES snapshots(id),
    prev_hash          TEXT NOT NULL DEFAULT '0000000000000000000000000000000000000000000000000000000000000000',
    redacted_diff_json TEXT,
    redaction_sites    INTEGER NOT NULL DEFAULT 0,
    outcome            TEXT NOT NULL CHECK (outcome IN ('ok', 'error', 'denied')),
    error_kind         TEXT,
    notes              TEXT
);
CREATE INDEX audit_log_correlation_id ON audit_log(correlation_id);
CREATE INDEX audit_log_occurred_at ON audit_log(occurred_at);
CREATE INDEX audit_log_actor_id ON audit_log(actor_id);
CREATE INDEX audit_log_kind ON audit_log(kind);

CREATE TABLE proposals (
    id                 TEXT PRIMARY KEY,
    caddy_instance_id  TEXT NOT NULL DEFAULT 'local',
    correlation_id     TEXT NOT NULL,
    source             TEXT NOT NULL CHECK (source IN ('docker', 'llm', 'import')),
    source_ref         TEXT,
    payload_json       TEXT NOT NULL,
    rationale          TEXT,
    submitted_at       INTEGER NOT NULL,
    expires_at         INTEGER NOT NULL,
    state              TEXT NOT NULL CHECK (state IN ('pending', 'approved', 'rejected', 'expired', 'superseded')),
    wildcard_callout   INTEGER NOT NULL DEFAULT 0,
    decided_by_kind    TEXT,
    decided_by_id      TEXT,
    decided_at         INTEGER,
    wildcard_ack_by    TEXT,
    wildcard_ack_at    INTEGER,
    resulting_mutation TEXT
);
CREATE INDEX proposals_state ON proposals(state);
CREATE INDEX proposals_submitted_at ON proposals(submitted_at);
CREATE INDEX proposals_correlation_id ON proposals(correlation_id);

CREATE TABLE mutations (
    id                  TEXT PRIMARY KEY,
    caddy_instance_id   TEXT NOT NULL DEFAULT 'local',
    correlation_id      TEXT NOT NULL,
    submitted_by_kind   TEXT NOT NULL,
    submitted_by_id     TEXT NOT NULL,
    submitted_at        INTEGER NOT NULL,
    expected_version    INTEGER NOT NULL,
    payload_json        TEXT NOT NULL,
    state               TEXT NOT NULL CHECK (state IN ('queued', 'validating', 'applying', 'applied', 'rejected', 'failed')),
    state_changed_at    INTEGER NOT NULL,
    result_snapshot_id  TEXT REFERENCES snapshots(id),
    failure_kind        TEXT,
    failure_message     TEXT
);
CREATE INDEX mutations_state ON mutations(state);
CREATE INDEX mutations_correlation_id ON mutations(correlation_id);

CREATE TABLE secrets_metadata (
    id                TEXT PRIMARY KEY,
    caddy_instance_id TEXT NOT NULL DEFAULT 'local',
    owner_kind        TEXT NOT NULL,
    owner_id          TEXT NOT NULL,
    field_path        TEXT NOT NULL,
    nonce             BLOB NOT NULL,
    ciphertext        BLOB NOT NULL,
    created_at        INTEGER NOT NULL,
    rotated_at        INTEGER,
    last_revealed_at  INTEGER,
    last_revealed_by  TEXT
);
```

The `migrations/README.md` file MUST state, verbatim:

> Migrations are up-only. A migration that has been applied to any production database MUST NOT be edited. Each schema change is a new migration. Down migrations are not provided in V1.
>
> **PRAGMA application_id:** The `PRAGMA application_id = 0x54525754` is set programmatically by the daemon after successful migration (in `crates/cli/src/main.rs`), not in the DDL. This ensures the pragma is atomic with schema initialization: if a daemon crashes mid-migration, the database remains in a consistent state (incomplete schema or no pragma, never a mismatch).
>
> **Advisory lock limitation:** The daemon acquires an advisory file lock at `<data_dir>/trilithon.lock` to prevent multiple daemons from accessing the same database. This lock is advisory only: it does NOT prevent direct access via the `sqlite3` CLI, backup scripts, or other processes that do not check the lock file. Operators MUST ensure no other process writes to the database file while the daemon is running. The daemon validates the database's `PRAGMA application_id` to provide additional protection against accidentally opening the wrong file.

### Algorithm

DDL only. Subsequent slices wire `sqlx::migrate!` to apply this file.

### Tests

The tests for this slice exercise the file at parse time and apply time; they live in slice 2.4 once `SqliteStorage` exists. This slice's standalone test:

- `core/crates/adapters/tests/migrations_parse.rs::initial_schema_parses` — uses `sqlx::migrate::Migrator::new` against the migrations directory and asserts no parse error. Also asserts the migrator's `.iter().count() == 1`.

### Acceptance command

```
cargo test -p trilithon-adapters --test migrations_parse
```

### Exit conditions

- `0001_init.sql` exists with eight tables: `caddy_instances`, `users`, `sessions`, `snapshots`, `audit_log`, `mutations`, `proposals`, `secrets_metadata`. The `_sqlx_migrations` table is created by sqlx at runtime, not in the DDL.
- Every data table carries `caddy_instance_id TEXT NOT NULL DEFAULT 'local'`.
- The migration file parses via `sqlx`.
- `migrations/README.md` documents the up-only policy.

### Audit kinds emitted

None at the schema level. The `audit_log.kind` column is populated by Phase 6 writers.

### Tracing events emitted

None.

### Cross-references

- Architecture §6.1–§6.10.
- ADR-0006, ADR-0009.
- Phase reference: "Author migration `0001_init.sql`", "Add a `caddy_instance_id` default of `local` everywhere".

---

## Slice 2.4 [cross-cutting] — `SqliteStorage` adapter, pragmas, advisory lock

### Goal

Implement `SqliteStorage` over `sqlx`. Pool initialisation MUST execute `PRAGMA journal_mode = WAL`, `PRAGMA synchronous = NORMAL`, `PRAGMA foreign_keys = ON`, `PRAGMA busy_timeout = 5000`. An advisory file lock at `<data_dir>/trilithon.lock` prevents two daemons from opening the same database. The adapter implements every method on `Storage`.

### Entry conditions

- Slices 2.1, 2.2, 2.3 complete.
- `crates/adapters/Cargo.toml` declares `sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "macros", "migrate"] }`, `tokio = { version = "1", features = ["full"] }`, `fs2 = "0.4"` (advisory file lock), `async-trait = "0.1"`.

### Files to create or modify

- `core/crates/adapters/src/sqlite_storage.rs` — adapter (new).
- `core/crates/adapters/src/lock.rs` — advisory lock helper (new).
- `core/crates/adapters/src/lib.rs` — `pub mod sqlite_storage; pub mod lock;` (modify).
- `core/crates/adapters/tests/sqlite_storage.rs` — integration tests (new).

### Signatures and shapes

```rust
// core/crates/adapters/src/sqlite_storage.rs
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use trilithon_core::storage::{Storage, StorageError, types::*, error::SqliteErrorKind};

pub struct SqliteStorage {
    pool:           SqlitePool,
    _lock_handle:   crate::lock::LockHandle,
    data_dir:       PathBuf,
}

impl SqliteStorage {
    pub async fn open(data_dir: &Path) -> Result<Self, StorageError>;
    pub fn pool(&self) -> &SqlitePool { &self.pool }
}

#[async_trait::async_trait]
impl Storage for SqliteStorage { /* every method */ }
```

```rust
// core/crates/adapters/src/lock.rs
use std::fs::File;
use std::path::Path;
use fs2::FileExt;

pub struct LockHandle { file: File }

impl LockHandle {
    /// Acquire an exclusive advisory lock on `<dir>/trilithon.lock`. Fails fast
    /// if a different process already holds the lock.
    pub fn acquire(dir: &Path) -> Result<Self, LockError>;
}

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("another Trilithon may be running (lock held on {path})")]
    AlreadyHeld { path: std::path::PathBuf },
    #[error("io error acquiring lock: {source}")]
    Io { #[source] source: std::io::Error },
}
```

### Algorithm

`SqliteStorage::open(data_dir)` MUST:

1. Acquire `LockHandle::acquire(data_dir)`. Failure → `StorageError::Io { source }` (mapping `LockError::AlreadyHeld` into a documented error chain). The CLI maps this to exit code `3`.
2. Build `SqliteConnectOptions`:
   - Use `.filename(data_dir.join("trilithon.db"))` instead of string formatting to safely handle spaces and special characters.
   - `.create_if_missing(true)`
   - `.journal_mode(SqliteJournalMode::Wal)`
   - `.synchronous(SqliteSynchronous::Normal)`
   - `.foreign_keys(true)`
   - `.busy_timeout(Duration::from_millis(config.storage.busy_timeout_ms))` — read from `RuntimeConfig`; default 5000 ms.
3. `SqlitePoolOptions::new().max_connections(10).connect_with(opts).await`.
4. Read `PRAGMA application_id` and validate it: if it equals `0` (brand-new SQLite file), proceed; if it equals `0x54525754`, proceed; otherwise return `StorageError::Sqlite { kind: Other("application_id mismatch; wrong database file") }`. This defends against accidentally opening the wrong file while allowing fresh installs.
5. Return `SqliteStorage { pool, _lock_handle, data_dir: data_dir.to_owned() }`.

Note: The `PRAGMA application_id = 0x54525754` is set programmatically in the daemon startup sequence (slice 2.7) after migrations complete, not inside the migration DDL. This ensures the pragma is atomic with successful schema initialization.

Per-method implementations:

- `insert_snapshot` — (1) `BEGIN IMMEDIATE` (ensures serialized access, preventing concurrent version conflicts). (2) Use `SELECT COALESCE(MAX(config_version), 0) FROM snapshots WHERE caddy_instance_id = ?` into `max_version` to ensure the result is never SQL NULL (empty table returns 0, not NULL). The first valid snapshot has `config_version = 1`. (3) Verify caller's `config_version == max_version + 1`; if not, `ROLLBACK` and return `StorageError::Integrity { detail: "config_version mismatch" }`. (4) `INSERT INTO snapshots` with all fields as bound parameters. If the INSERT succeeds, `COMMIT` and return the id. If a UNIQUE constraint violation on `(caddy_instance_id, config_version)` occurs, `ROLLBACK` and return `StorageError::SnapshotDuplicate { id }` (this should not occur under `BEGIN IMMEDIATE` unless there is a concurrent bug, but catch it defensively). Idempotent retries with the same payload are handled at the caller level (mutation queue, Phase 5) by checking the `Integrity` error code and re-reading the current max version before retrying.
- `get_snapshot` — single `SELECT ... WHERE id = ?`.
- `parent_chain` — (1) Assert `max_depth <= 256`; return `StorageError::Integrity { detail: "max_depth exceeds 256" }` if violated. (2) Recursive CTE walking `parent_id WITH RECURSIVE ... LIMIT :max_depth`, ordered from oldest (root) to newest (leaf). Return `ParentChain { snapshots, truncated: (count == max_depth) }`.
- `latest_desired_state` — `SELECT ... ORDER BY config_version DESC LIMIT 1`.
- `record_audit_event` — wrap in `BEGIN IMMEDIATE` transaction. (1) Validate `event.kind` against the §6.6 vocabulary list; reject with `AuditKindUnknown` if unknown. (2) To compute `prev_hash`: query `SELECT * FROM audit_log ORDER BY occurred_at DESC, id DESC LIMIT 1` to fetch the previous row. The secondary `id DESC` ensures a deterministic choice when multiple rows share the same `occurred_at` (which is routine — multiple events in the same second). ULIDs are monotonically increasing, so `id DESC` selects the insertion-order predecessor. If a row exists, call `canonical_json_for_hash(row)` to serialize it and compute `SHA256(canonical_json)`; set `event.prev_hash`. If no prior row exists, set `event.prev_hash = "0000000000000000000000000000000000000000000000000000000000000000"` (64 zeros). The helper is defined in `core::storage::helpers` and both adapters call it. (3) `INSERT INTO audit_log` with all fields (including the computed `prev_hash`) as bound parameters; never concatenate user input. (4) `COMMIT`. This ensures the hash chain is built correctly and deterministically from the first row (ADR-0009).
- `tail_audit_log` — dynamically built `WHERE` clause with bound parameters for all `AuditSelector` fields (`kind_glob`, `actor_id`, `correlation_id`, `since`, `until`); `kind_glob` uses `LIKE ? ESCAPE '\'` if present. No string concatenation. Order by `occurred_at DESC LIMIT ?`.
- `record_drift_event` — stub for Phase 2. `drift_events` table does not appear in `0001_init.sql` (Phase 8). Return `StorageError::Sqlite { kind: SqliteErrorKind::Other("drift_events table not yet available; implement in Phase 8".into()) }` to indicate feature unavailability (distinct from a real migration failure).
- `latest_drift_event` — same caveat.
- `enqueue_proposal`, `dequeue_proposal`, `expire_proposals` — proposals table arrives in `0001_init.sql` now (all columns per architecture §6.8). `dequeue_proposal` is destructive: (1) `BEGIN IMMEDIATE`. (2) `SELECT * FROM proposals WHERE state = 'pending' ORDER BY submitted_at ASC LIMIT 1` and hold the row data. (3) If found, attempt `DELETE FROM proposals WHERE id = ?`. Check the result: if `rows_affected > 0`, `COMMIT` and return the row. If `rows_affected == 0` (the row disappeared between SELECT and DELETE, possibly deleted by external `sqlite3` CLI process, which bypasses the advisory lock), `ROLLBACK` and return `Err(StorageError::Sqlite { kind: Other("proposal disappeared between select and delete; possible external modification".into()) })` to prevent silent double-processing. If the DELETE sqlx query itself fails with a sqlx error (e.g., constraint violation, SQLITE_BUSY after timeout), `ROLLBACK` and return `Err(StorageError::Sqlite { kind: ... })`. If no pending proposal exists in step 2, return `None`. This avoids undefined state transitions and prevents double-processing.

### Tests

- `core/crates/adapters/tests/sqlite_storage.rs::pragmas_applied_after_open` — opens a temp DB, queries `PRAGMA journal_mode`, `PRAGMA synchronous`, `PRAGMA foreign_keys`, `PRAGMA busy_timeout`; asserts `wal`, `1`, `1`, and the configured timeout (default `5000`) respectively.
- `tests::application_id_fresh_db_allowed` — opens a brand-new temp DB (application_id = 0 by default), calls `SqliteStorage::open` before migrations, asserts success (fresh databases with application_id = 0 are allowed).
- `tests::application_id_correct_accepted` — opens a temp DB, manually sets `PRAGMA application_id = 0x54525754`, then calls `SqliteStorage::open`, asserts success.
- `tests::application_id_wrong_rejected` — manually creates a DB with `PRAGMA application_id = 0x12345678` (wrong value), attempts `SqliteStorage::open`, asserts `Sqlite { kind: Other(...) }` error mentioning "application_id mismatch".
- `tests::insert_get_snapshot_round_trip`.
- `tests::insert_duplicate_same_body_idempotent`.
- `tests::insert_duplicate_different_body_returns_duplicate_error`.
- `tests::record_audit_event_known_kind_succeeds` — kind `"config.applied"`.
- `tests::record_audit_event_unknown_kind_rejected` — kind `"made.up"` → `AuditKindUnknown`.
- `tests::record_audit_event_first_prev_hash_is_zero` — inserts first event, asserts `prev_hash == "0000000000000000000000000000000000000000000000000000000000000000"`.
- `tests::record_audit_event_chains_prev_hash` — inserts two events, asserts second event's `prev_hash == SHA256(canonical_json(first_event))` and is not all-zero.
- `tests::tail_audit_log_filters_by_correlation_id`.
- `tests::tail_audit_log_sql_injection_rejected` — inserts a known audit row, then calls `tail_audit_log` with `kind_glob = "'; DROP TABLE audit_log; --"`, asserts the query returns zero rows (the injection is safely escaped) rather than executing the DROP statement.
- `tests::dequeue_proposal_removes_from_queue` — inserts a pending proposal, calls `dequeue_proposal`, asserts it returns the proposal and a second call returns `None` (destructive dequeue).
- `tests::dequeue_proposal_propagates_delete_error` — inserts a pending proposal, then simulates DELETE failure (e.g., by dropping the proposals table between SELECT and DELETE using a second raw SQLite connection), calls `dequeue_proposal`, asserts it returns `Err(StorageError::Sqlite { ... })` and does not return the proposal row.
- `tests::dequeue_proposal_rows_affected_zero_returns_error` — inserts a pending proposal, then uses a second raw connection to manually `DELETE FROM proposals WHERE id = <selected_id>` between the SELECT and DELETE steps of `dequeue_proposal`, calls `dequeue_proposal`, asserts it returns `Err(StorageError::Sqlite { ... })` with the "disappeared" message, preventing silent double-processing.
- `tests::advisory_lock_rejects_second_open` — opens, then attempts `SqliteStorage::open` again on the same dir, asserts the second open errors with the lock-already-held message.
- `tests::parent_chain_max_depth_enforced` — calls `parent_chain` with `max_depth = 1000`, asserts `Integrity { detail: "exceeds 256" }`.
- `tests::parent_chain_self_cycle_prevented` — attempts to insert a snapshot with `parent_id = id` (self-cycle), asserts `Sqlite { kind: Constraint }` from the `CHECK` constraint.

### Acceptance command

```
cargo test -p trilithon-adapters --test sqlite_storage
```

### Exit conditions

- All eight named tests pass.
- `SqliteStorage::open` returns within 200 ms on a clean temp directory.
- Pragmas are observable on a freshly opened pool.

### Audit kinds emitted

`config.applied` and `mutation.rejected.missing-expected-version` are exercised by tests but are not emitted by Trilithon code at this slice; the records are inserted as test inputs.

### Tracing events emitted

None at this slice; integration with `daemon.started` happens in slice 2.7.

### Cross-references

- Trait signatures §1.
- Architecture §6, §10 (H14).
- ADR-0006.

### Open questions surfaced

1. **Resolved by adversarial round 1:** The `proposals` table is now included in `0001_init.sql` with all columns from architecture §6.8. The `caddy_instances` field appears on all data tables; the schema now reflects this consistently. Two columns (`created_at_monotonic_nanos`, `canonical_json_version`) were added to `snapshots` per architecture §6.5. The `prev_hash` column was added to `audit_log` per ADR-0009.
2. `drift_events`, `tokens`, `policy_presets`, `route_policy_attachments`, `capability_probe_results`, and `routes` are documented in architecture §6 but not in `0001_init.sql`. These are added in their owning phases (Phase 3, 4, 8, 18, 19). The phase reference should confirm which tables belong to which phases.

---

## Slice 2.5 [standard] — Migration runner with downgrade refusal

### Goal

Run embedded migrations on daemon startup via `sqlx::migrate!`. If the database carries a `schema_migrations.version` newer than the embedded set, refuse to start with exit code `3`. Emit `storage.migrations.applied` on success.

### Entry conditions

- Slice 2.4 complete.

### Files to create or modify

- `core/crates/adapters/src/migrate.rs` — runner (new).
- `core/crates/adapters/src/lib.rs` — re-export (modify).

### Signatures and shapes

```rust
// core/crates/adapters/src/migrate.rs
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn apply_migrations(pool: &SqlitePool) -> Result<MigrationOutcome, MigrationError>;

#[derive(Debug)]
pub struct MigrationOutcome {
    pub applied_count: u32,
    pub current_version: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("database schema version {db_version} is newer than embedded set max {embedded_max}; refusing to start")]
    Downgrade { db_version: u32, embedded_max: u32 },
    #[error("migration failure: {source}")]
    Sqlx { #[from] source: sqlx::migrate::MigrateError },
}
```

### Algorithm

1. Query `SELECT MAX(version) FROM _sqlx_migrations`. If the table does not exist, treat as `0`. Note: `sqlx::migrate!` writes to `_sqlx_migrations` by default, NOT to the custom `schema_migrations` table.
2. Read `MIGRATOR.iter().map(|m| m.version).max()` as `embedded_max`.
3. If `db_version > embedded_max`, return `MigrationError::Downgrade`.
4. Otherwise call `MIGRATOR.run(pool).await` and count newly applied migrations by diffing the row count on `_sqlx_migrations` before and after.
5. Emit `tracing::info!("storage.migrations.applied", current_version = outcome.current_version, applied = outcome.applied_count)`.

### Tests

- `core/crates/adapters/tests/migrate.rs::fresh_db_applies_all` — opens an empty temp DB, runs `apply_migrations`, asserts `applied_count >= 1` and `current_version == 1`.
- `core/crates/adapters/tests/migrate.rs::idempotent_second_run` — runs twice, asserts second run reports `applied_count == 0`.
- `core/crates/adapters/tests/migrate.rs::refuses_downgrade` — manually inserts `(version, installed_on, description, installed_by, execution_time, success)` tuple into `_sqlx_migrations` with `version=999`, runs `apply_migrations`, asserts `Err(Downgrade { db_version: 999, embedded_max: 1 })`.

### Acceptance command

```
cargo test -p trilithon-adapters --test migrate
```

### Exit conditions

- `storage.migrations.applied` event is observable in test logs.
- A future-versioned database produces `MigrationError::Downgrade`, which the CLI maps to exit `3`.

### Audit kinds emitted

None at this slice. Phase 6 may decide to log `caddy.capability-probe-completed`-style events around migration; not in scope.

### Tracing events emitted

- `storage.migrations.applied` (architecture §12.1).

### Cross-references

- Architecture §10, §12.1.
- Phase reference: "Embed migrations and run them at startup", "Refuse downgrade migrations".

---

## Slice 2.6 [cross-cutting] — Periodic `PRAGMA integrity_check` task

### Goal

Run an integrity check on daemon startup (before emitting `daemon.started`), then spawn a periodic task that runs `PRAGMA integrity_check` every 6 hours. Any non-`ok` result emits `storage.integrity-check.failed` with a redacted detail. Startup check failure exits `3`. Per H14 (architecture §10), the periodic task survives daemon shutdown via the `ShutdownObserver` plumbing from slice 1.5.

### Entry conditions

- Slice 2.4 complete.
- Slice 1.5's `ShutdownSignal` is available.
- `crates/core/Cargo.toml` includes `pub mod lifecycle;` in `crates/core/src/lib.rs`.

### Files to create or modify

- `core/crates/core/src/lifecycle.rs` — `ShutdownObserver` trait (new).
- `core/crates/adapters/src/integrity_check.rs` — task (new).
- `core/crates/adapters/src/lib.rs` — re-export (modify).

### Signatures and shapes

```rust
// core/crates/core/src/lifecycle.rs
pub trait ShutdownObserver: Send {
    async fn wait(&mut self);
}
```

```rust
// core/crates/adapters/src/integrity_check.rs
use sqlx::SqlitePool;
use std::time::Duration;
use tokio::time::{interval, MissedTickBehavior};
use trilithon_core::lifecycle::ShutdownObserver;

pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub async fn run_integrity_loop(
    pool: SqlitePool,
    every: Duration,
    mut shutdown: impl ShutdownObserver,
) -> ();

pub async fn integrity_check_once(pool: &SqlitePool) -> Result<IntegrityResult, sqlx::Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityResult { Ok, Failed { detail: String } }
```

This slice MUST define `core::lifecycle::ShutdownObserver` — a pure async trait with a single `async fn wait(&mut self)` method and zero dependencies. The trait lives in `crates/core/src/lifecycle.rs`. The `tokio::sync::watch`-backed implementation (`adapters::shutdown::WatchShutdownObserver`) lives in `adapters` behind a `feature` gate or directly in `adapters::integrity_check::run_integrity_loop`'s caller (slice 2.7). The `ShutdownSignal` type in `crates/cli/src/shutdown.rs` (slice 1.5) implements `ShutdownObserver` by exposing a method that wraps the watch channel.

### Algorithm

1. **Startup check (called from slice 2.7 before `daemon.started`):** Call `integrity_check_once(&pool)` synchronously after opening storage AND after migrations complete (but before `daemon.started` is emitted). This validates that the migrated schema is structurally sound. If `Failed { detail }`, return `StorageError::Integrity { detail }`, which maps to exit code `3` in slice 2.7.
2. **Periodic loop (spawned after successful startup):** Build `tokio::time::interval(every)` with `MissedTickBehavior::Skip`.
3. Loop:
   1. `tokio::select!` on `interval.tick()` and `shutdown.wait()`.
   2. On tick: run `integrity_check_once(&pool)`; if `Failed { detail }`, `tracing::error!("storage.integrity-check.failed", detail = %detail)`.
   3. On shutdown: break.
4. `integrity_check_once` issues `PRAGMA integrity_check` and reads the first row. The string `"ok"` maps to `IntegrityResult::Ok`; anything else maps to `Failed { detail: row.0 }`.

### Tests

- `core/crates/adapters/tests/integrity_check.rs::healthy_db_reports_ok` — opens fresh DB, runs `integrity_check_once`, asserts `Ok`.
- `core/crates/adapters/tests/integrity_check.rs::shutdown_breaks_the_loop` — spawns `run_integrity_loop` with a 100 ms interval, triggers shutdown after 50 ms, asserts the future resolves within 250 ms.
- `core/crates/adapters/src/integrity_check.rs::tests::failed_result_emits_event` — uses `tracing_test::traced_test` to assert that a synthesised `Failed { detail: "page 1 corrupt" }` produces the `storage.integrity-check.failed` event with the documented field key.

### Acceptance command

```
cargo test -p trilithon-adapters integrity_check
```

### Exit conditions

- The integrity task emits `storage.integrity-check.failed` on a non-`ok` result.
- The task terminates when the shutdown signal fires.

### Audit kinds emitted

None at this slice; the storage-corruption audit kind is OUT OF SCOPE FOR V1 per §6.6.

### Tracing events emitted

- `storage.integrity-check.failed` (architecture §12.1).

### Cross-references

- Architecture §10 (H14), §12.1.
- Phase reference: "Periodic `PRAGMA integrity_check` task".

### Open questions surfaced

**None.** Open Question 3 (ShutdownSignal layer isolation) is resolved in slice 2.6 by defining `core::lifecycle::ShutdownObserver` as a pure trait (option a), with the `tokio::sync::watch` implementation in `adapters`.

---

## Slice 2.7 [cross-cutting] — Wire startup, exit code 3 on storage failure, integration tests

### Goal

Wire `SqliteStorage::open` and `apply_migrations` into the daemon's startup path. Failure at either step exits `3`. Emit `daemon.started` only after migrations succeed. Add `core/README.md` "Persistence" documentation.

### Entry conditions

- Slices 2.1 through 2.6 complete.

### Files to create or modify

- `core/crates/cli/src/main.rs` — open storage, run migrations, spawn integrity task (modify).
- `core/crates/cli/src/exit.rs` — add `From<StorageError>` and `From<MigrationError>` mapping into `ExitCode::StartupPreconditionFailure` (modify).
- `core/crates/cli/tests/storage_startup.rs` — integration tests (new).
- `core/README.md` — "Persistence" section (modify).

### Signatures and shapes

```rust
// core/crates/cli/src/main.rs (excerpt, in run handler)
let storage = trilithon_adapters::sqlite_storage::SqliteStorage::open(&config.storage.data_dir)
    .await
    .map_err(|e| { tracing::error!(error.kind = ?e, "storage.open.failed"); ExitCode::StartupPreconditionFailure })?;

let outcome = trilithon_adapters::migrate::apply_migrations(storage.pool())
    .await
    .map_err(|e| { tracing::error!(error.kind = ?e, "migration.failed"); ExitCode::StartupPreconditionFailure })?;
tracing::info!(version = outcome.current_version, applied = outcome.applied_count, "storage.migrations.applied");

// Set PRAGMA application_id after migrations complete, ensuring atomicity with schema initialization
// This prevents a partial-init state where application_id is set but tables do not exist
sqlx::query("PRAGMA application_id = ?")
    .bind(0x54525754_i64)
    .execute(storage.pool())
    .await
    .map_err(|e| { tracing::error!(error = ?e, "storage.pragma_application_id.failed"); ExitCode::StartupPreconditionFailure })?;

// Startup integrity check (ADR-0006, H14 requirement) — MUST run before daemon.started
let integrity_result = trilithon_adapters::integrity_check::integrity_check_once(storage.pool())
    .await
    .map_err(|e| { tracing::error!(error = ?e, "storage.integrity-check.startup.io.failed"); ExitCode::StartupPreconditionFailure })?;
if let trilithon_adapters::integrity_check::IntegrityResult::Failed { detail } = integrity_result {
    tracing::error!(detail = %detail, "storage.integrity-check.startup.corruption.detected");
    return Err(ExitCode::StartupPreconditionFailure);
}

let storage = std::sync::Arc::new(storage);
let pool = storage.pool().clone();

// Build a ShutdownObserver impl from the shutdown signal (from slice 1.5)
let shutdown_observer = /* construct impl ShutdownObserver from shutdown signal */;
tokio::spawn(trilithon_adapters::integrity_check::run_integrity_loop(
    pool,
    trilithon_adapters::integrity_check::DEFAULT_INTERVAL,
    shutdown_observer,
));

tracing::info!("daemon.started");
```

The `core/README.md` "Persistence" section MUST cover:

- WAL mode and the four pragmas.
- The migration directory at `crates/adapters/migrations/` and its up-only policy.
- The 6-hour integrity check schedule.
- Reference to ADR-0006.

### Algorithm

1. After config load and tracing init, before `daemon.started`:
   1. Open `SqliteStorage`.
   2. Apply migrations.
   3. Spawn integrity task.
2. Any of the above failing exits `3` with a typed message.
3. Hold the `SqliteStorage` `Arc` for the daemon's lifetime so the advisory lock persists.

### Tests

- `core/crates/cli/tests/storage_startup.rs::missing_data_dir_exits_3` — points config at `/nonexistent`, asserts exit `3` and stderr contains `storage`.
- `core/crates/cli/tests/storage_startup.rs::successful_startup_emits_migrations_applied` — runs the binary against a temp dir for ~500 ms in JSON log mode, captures stderr, asserts presence of an event with name `storage.migrations.applied` and a numeric `version` field.
- `core/crates/cli/tests/storage_startup.rs::application_id_set_after_startup` — after successful daemon startup against a temp dir, directly queries the database file (via a separate sqlx::connect to the same file) and asserts `PRAGMA application_id` returns `0x54525754`, verifying the pragma was set post-migration.
- `core/crates/cli/tests/storage_startup.rs::second_daemon_against_same_dir_exits_3` — runs two binaries pointed at the same dir, asserts the second exits `3` and stderr mentions the lock.

### Acceptance command

```
cargo test -p trilithon-cli --test storage_startup
```

### Exit conditions

- `daemon.started` is emitted only after migrations succeed.
- Storage open failure or migration failure exits `3`.
- A second daemon against the same `data_dir` exits `3`.
- The `core/README.md` "Persistence" section exists.
- All three named tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

- `daemon.started`, `storage.migrations.applied`, `storage.integrity-check.failed` (when applicable).

### Cross-references

- Architecture §6, §10, §12.1.
- ADR-0006, ADR-0009.
- Phase reference §"Sign-off checklist".

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] The daemon runs an integrity check on startup (synchronously, before `daemon.started`) and exits `3` on corruption. The periodic 6-hour task is spawned independently.
- [ ] The daemon runs migrations on startup and emits `storage.migrations.applied` with the resulting schema version.
- [ ] The daemon exits `3` if SQLite cannot acquire the database file, if migrations fail, if `PRAGMA application_id` does not match `0x54525754`, or if the startup integrity check fails.
- [ ] All nine Tier 1 + meta tables exist after first run (caddy_instances, users, sessions, snapshots, audit_log, mutations, proposals, secrets_metadata, _sqlx_migrations). The dead `schema_migrations` table is removed.
- [ ] Audit log rows carry a `prev_hash` column and are correctly written by computing the previous row's hash and setting the field (not just relying on the DEFAULT) per ADR-0009.
- [ ] Snapshots carry `created_at_monotonic_nanos` and `canonical_json_version` per architecture §6.5.
- [ ] `snapshot.parent_id` has a `CHECK (parent_id != id)` constraint to prevent self-cycles.
- [ ] `parent_chain` enforces `max_depth <= 256`; exceeding this returns `StorageError::Integrity`.
- [ ] `record_audit_event` wraps the insert in `BEGIN IMMEDIATE` and correctly computes `prev_hash` before inserting.
- [ ] `insert_snapshot` wraps read-then-insert in `BEGIN IMMEDIATE` to prevent concurrent version conflicts.
- [ ] `tail_audit_log` uses bound parameters for all `AuditSelector` fields (no string concatenation). SQL injection test passes.
- [ ] `config_version` increment uses `BEGIN IMMEDIATE` transactions.
- [ ] `busy_timeout` is configurable via `RuntimeConfig`; pragma is set once, not doubled.
- [ ] `core::lifecycle::ShutdownObserver` trait is defined in `crates/core/src/lifecycle.rs` with zero dependencies.
- [ ] `InMemoryStorage` uses `tokio::sync::Mutex`, not `std::sync::Mutex`.
- [ ] Unimplemented methods (`record_drift_event`, `latest_drift_event`) return `StorageError::Sqlite { kind: Other(...) }` with a "not yet available" message, not `Migration` error.
- [ ] A second daemon process pointed at the same database file is rejected before any write occurs.

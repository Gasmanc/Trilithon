//! Storage trait surface: trait definition, supporting types, and errors.

pub mod audit_vocab;
pub mod error;
pub mod helpers;
pub mod trait_def;
pub mod types;

#[cfg(test)]
pub mod in_memory;

/// Fixed `PRAGMA application_id` that every Trilithon database must carry.
///
/// Value `0x5452_5754` = 1 414 681 940 (ASCII `"TRWT"` — Trilithon).
/// Set by migration `0005_application_id.sql`, verified at startup (ADR-0006).
pub const APPLICATION_ID: u32 = 0x5452_5754;

/// Strip a trailing `*` from a glob pattern, returning the prefix.
/// If the pattern does not end with `*`, returns `None` (exact match).
pub fn glob_prefix(pattern: &str) -> Option<&str> {
    pattern.strip_suffix('*')
}

pub use error::StorageError;
pub use trait_def::Storage;
pub use types::{
    ActorKind, AuditEventRow, AuditOutcome, AuditRowId, AuditSelector, DriftEventRow,
    DriftResolution, DriftRowId, ParentChain, ProposalId, ProposalRow, ProposalSource,
    ProposalState, Snapshot, SnapshotId, StorageAuditEvent, UnixSeconds,
};

//! `StorageError` — the error type for all `Storage` trait methods.

use crate::storage::types::SnapshotId;

/// All errors that can be returned by `Storage` implementations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// A row failed its content-hash integrity check.
    #[error("integrity check failed: {detail}")]
    Integrity {
        /// Human-readable description of the check that failed.
        detail: String,
    },

    /// An audit event was submitted with a `kind` string outside the §6.6 vocabulary.
    #[error("audit kind {kind} is not in the §6.6 vocabulary")]
    AuditKindUnknown {
        /// The unrecognised kind string.
        kind: String,
    },

    /// An attempt was made to insert a snapshot whose id already exists.
    #[error("snapshot {id:?} already exists")]
    SnapshotDuplicate {
        /// The conflicting snapshot identifier.
        id: SnapshotId,
    },

    /// The parent snapshot referenced by `parent_id` does not exist.
    #[error("parent snapshot {parent_id:?} does not exist")]
    SnapshotParentNotFound {
        /// The parent identifier that could not be resolved.
        parent_id: SnapshotId,
    },

    /// The `config_version` is not strictly greater than the current maximum.
    #[error("config_version {attempted} is not greater than current max {current_max}")]
    SnapshotVersionNotMonotonic {
        /// The `config_version` that was attempted.
        attempted: i64,
        /// The current maximum `config_version` for this instance.
        current_max: i64,
    },

    /// Two distinct snapshots produced the same SHA-256 hash (collision).
    ///
    /// This is treated as a fatal integrity failure; the caller should abort.
    #[error("SHA-256 collision detected for snapshot id {id:?}")]
    SnapshotHashCollision {
        /// The colliding snapshot identifier.
        id: SnapshotId,
    },

    /// An open proposal with the same `(source, source_ref)` already exists.
    ///
    /// The field is named `proposal_source` rather than `source` to avoid the
    /// `thiserror` v2 implicit-source-field heuristic, which requires the field
    /// type to implement `std::error::Error`.
    #[error("proposal duplicate for ({proposal_source}, {source_ref})")]
    ProposalDuplicate {
        /// The source system that submitted the duplicate proposal.
        proposal_source: String,
        /// The source-system-specific reference identifier.
        source_ref: String,
    },

    /// The `SQLite` connection was busy and could not be acquired.
    ///
    /// Phase 4 will re-add retry tracking when the mutation queue retry loop lands.
    #[error("sqlite busy")]
    SqliteBusy,

    /// A low-level `SQLite` error occurred.
    #[error("sqlite error: {kind:?}")]
    Sqlite {
        /// Classification of the `SQLite` error.
        kind: SqliteErrorKind,
    },

    /// A schema migration step failed.
    #[error("schema migration {version} failed: {detail}")]
    Migration {
        /// Migration version number that failed.
        version: u32,
        /// Human-readable description of the failure.
        detail: String,
    },

    /// A method is structurally present on the trait but its backing schema or
    /// implementation has not yet been added in this phase.
    ///
    /// Callers should treat this as a developer-wiring error, not a schema failure.
    #[error("storage feature not yet available: {reason}")]
    NotYetAvailable {
        /// Human-readable description of which feature is missing and when it lands.
        reason: String,
    },

    /// An underlying I/O error occurred.
    #[error("io error: {source}")]
    Io {
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// An optimistic-concurrency conflict was detected on `config_version`.
    ///
    /// The caller observed `expected` but the database holds `observed`.
    /// No write was made; the apply must be retried after re-fetching state.
    #[error("optimistic conflict: observed {observed}, expected {expected}")]
    OptimisticConflict {
        /// The `config_version` actually stored in the database.
        observed: i64,
        /// The `config_version` the caller expected to find.
        expected: i64,
    },
}

/// Classification of a low-level `SQLite` error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqliteErrorKind {
    /// A UNIQUE or FK constraint was violated.
    Constraint,
    /// The database file is locked by another process.
    Locked,
    /// The database file appears corrupted.
    Corrupt,
    /// Any other `SQLite` error; contains the raw error string.
    Other(String),
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;
    use std::fmt::Write as _;

    #[test]
    fn display_round_trip() {
        let variants: Vec<StorageError> = vec![
            StorageError::Integrity {
                detail: "hash mismatch".into(),
            },
            StorageError::AuditKindUnknown {
                kind: "bogus.event".into(),
            },
            StorageError::SnapshotDuplicate {
                id: SnapshotId("a".repeat(64)),
            },
            StorageError::SnapshotParentNotFound {
                parent_id: SnapshotId("b".repeat(64)),
            },
            StorageError::SnapshotVersionNotMonotonic {
                attempted: 3,
                current_max: 5,
            },
            StorageError::SnapshotHashCollision {
                id: SnapshotId("c".repeat(64)),
            },
            StorageError::ProposalDuplicate {
                proposal_source: "docker".into(),
                source_ref: "abc123".into(),
            },
            StorageError::SqliteBusy,
            StorageError::Sqlite {
                kind: SqliteErrorKind::Constraint,
            },
            StorageError::Migration {
                version: 3,
                detail: "column missing".into(),
            },
            StorageError::NotYetAvailable {
                reason: "feature lands in phase 4".into(),
            },
            StorageError::Io {
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "file missing"),
            },
            StorageError::OptimisticConflict {
                observed: 10,
                expected: 9,
            },
        ];

        for variant in variants {
            let mut buf = String::new();
            write!(buf, "{variant}").expect("Display should not fail");
            assert!(
                !buf.is_empty(),
                "Display output must be non-empty for {variant:?}"
            );
        }
    }
}

//! Shared sqlx error conversion utilities for adapters that wrap `SQLite`.

use trilithon_core::storage::error::{SqliteErrorKind, StorageError};

/// Substring present in every trigger-level abort message on the `audit_log`
/// table (architecture §6.6).
const AUDIT_IMMUTABLE_MSG: &str = "audit_log rows are immutable";

/// Convert a [`sqlx::Error`] into a typed [`StorageError`].
///
/// `SQLite` extended error codes are masked to the primary 8-bit code so that
/// variants like `SQLITE_BUSY_RECOVERY` (261) and `SQLITE_BUSY_SNAPSHOT` (517)
/// are caught alongside their base codes (5 = BUSY, 6 = LOCKED).
///
/// `RAISE(ABORT, ...)` from immutability triggers produces `SQLITE_ERROR` (1)
/// with the trigger message embedded in the error text; those are mapped to
/// [`StorageError::Integrity`] rather than the generic `Sqlite` variant.
#[allow(clippy::needless_pass_by_value, clippy::redundant_pub_crate)]
// zd:adapters-006 expires:2026-11-01 reason: pub(crate) mod + pub(crate) fn is intentional; makes visibility explicit at both levels
pub(crate) fn sqlx_err(e: sqlx::Error) -> StorageError {
    match &e {
        sqlx::Error::Database(db_err) => {
            let code: i32 = db_err.code().as_deref().unwrap_or("").parse().unwrap_or(0);
            let msg = db_err.message();
            // Detect immutability-trigger abort before the numeric code check:
            // RAISE(ABORT, message) sets code=1 (SQLITE_ERROR) and embeds the
            // message string in the error.
            if msg.contains(AUDIT_IMMUTABLE_MSG) {
                return StorageError::Integrity {
                    detail: msg.to_owned(),
                };
            }
            // Mask to the primary error code (low 8 bits) so that extended
            // codes such as SQLITE_BUSY_RECOVERY (261) and
            // SQLITE_BUSY_SNAPSHOT (517) are caught alongside the base codes.
            match code & 0xFF {
                5 | 6 => StorageError::SqliteBusy,
                11 => StorageError::Sqlite {
                    kind: SqliteErrorKind::Corrupt,
                },
                19 => StorageError::Sqlite {
                    kind: SqliteErrorKind::Constraint,
                },
                _ => StorageError::Sqlite {
                    kind: SqliteErrorKind::Other(e.to_string()),
                },
            }
        }
        _ => StorageError::Sqlite {
            kind: SqliteErrorKind::Other(e.to_string()),
        },
    }
}

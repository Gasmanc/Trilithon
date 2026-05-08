//! Shared sqlx error conversion utilities for adapters that wrap `SQLite`.

use trilithon_core::storage::error::{SqliteErrorKind, StorageError};

/// Convert a [`sqlx::Error`] into a typed [`StorageError`].
///
/// `SQLite` extended error codes are masked to the primary 8-bit code so that
/// variants like `SQLITE_BUSY_RECOVERY` (261) and `SQLITE_BUSY_SNAPSHOT` (517)
/// are caught alongside their base codes (5 = BUSY, 6 = LOCKED).
#[allow(clippy::needless_pass_by_value, clippy::redundant_pub_crate)]
// zd:adapters-006 expires:2026-11-01 reason: pub(crate) mod + pub(crate) fn is intentional; makes visibility explicit at both levels
pub(crate) fn sqlx_err(e: sqlx::Error) -> StorageError {
    match &e {
        sqlx::Error::Database(db_err) => {
            let code: i32 = db_err.code().as_deref().unwrap_or("").parse().unwrap_or(0);
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

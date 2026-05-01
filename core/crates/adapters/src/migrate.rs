//! Migration runner with downgrade refusal.
//!
//! Wraps `sqlx::migrate!` with a pre-flight check: if the database's recorded
//! schema version exceeds the highest version embedded in this binary, the
//! daemon refuses to start rather than silently corrupting the schema.

use sqlx::SqlitePool;
use sqlx::migrate::Migrator;

/// Embedded migration set, compiled relative to this crate's manifest directory.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Outcome reported after a successful migration run.
#[derive(Debug)]
pub struct MigrationOutcome {
    /// Number of migrations applied during this run (0 if already up to date).
    pub applied_count: u32,
    /// Highest migration version now present in the database.
    pub current_version: u32,
}

/// Errors that can occur while applying migrations.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    /// The database carries a schema version newer than the embedded set.
    #[error(
        "database schema version {db_version} is newer than embedded set max {embedded_max}; refusing to start"
    )]
    Downgrade {
        /// The version reported by the database.
        db_version: u32,
        /// The highest version present in the embedded migration set.
        embedded_max: u32,
    },

    /// A sqlx migration error occurred.
    #[error("migration failure: {source}")]
    Sqlx {
        /// The underlying sqlx migration error.
        #[from]
        source: sqlx::migrate::MigrateError,
    },
}

/// Apply pending migrations to `pool`, refusing if the database is ahead of the
/// embedded set.
///
/// # Errors
///
/// Returns [`MigrationError::Downgrade`] if the database carries a version
/// higher than the maximum version embedded in this binary.
///
/// Returns [`MigrationError::Sqlx`] if sqlx encounters an error while running
/// the migrations.
pub async fn apply_migrations(pool: &SqlitePool) -> Result<MigrationOutcome, MigrationError> {
    // Step 1 — determine the highest version the database has already seen.
    // We query `_sqlx_migrations`, which sqlx populates automatically.
    // If the table does not exist yet (fresh database), treat db_version as 0.
    let db_version: u32 =
        match sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
        {
            Ok(Some(v)) => u32::try_from(v).unwrap_or(0),
            // Table exists but has no rows, or table does not exist yet.
            Ok(None) | Err(_) => 0,
        };

    // Step 2 — determine the highest version embedded in this binary.
    let embedded_max: u32 = MIGRATOR
        .iter()
        .map(|m| m.version)
        .max()
        .map_or(0, |v| u32::try_from(v).unwrap_or(0));

    // Step 3 — refuse to start if the database is ahead.
    if db_version > embedded_max {
        return Err(MigrationError::Downgrade {
            db_version,
            embedded_max,
        });
    }

    // Step 4 — count rows before running, so we can compute applied_count.
    let count_before: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Run the migrations.
    MIGRATOR.run(pool).await?;

    // Count rows after.
    let count_after: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let applied_count = u32::try_from((count_after - count_before).max(0)).unwrap_or(0);

    // Step 5 — read the authoritative current version post-run.
    let current_version: u32 =
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .ok()
            .flatten()
            .map_or(0, |v| u32::try_from(v).unwrap_or(0));

    tracing::info!(
        current_version,
        applied = applied_count,
        "storage.migrations.applied"
    );

    Ok(MigrationOutcome {
        applied_count,
        current_version,
    })
}

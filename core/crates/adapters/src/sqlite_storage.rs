//! `SqliteStorage` — the SQLite-backed [`Storage`] adapter.
//!
//! Opens (or creates) `<data_dir>/trilithon.db` via `sqlx`, runs migrations,
//! and applies the required pragmas.  An advisory lock at
//! `<data_dir>/trilithon.lock` prevents two daemons from opening the same
//! database.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

use trilithon_core::storage::{
    audit_vocab::AUDIT_KINDS,
    error::{SqliteErrorKind, StorageError},
    helpers::{audit_prev_hash_seed, canonical_json_for_audit_hash, compute_audit_chain_hash},
    trait_def::Storage,
    types::{
        ActorKind, AuditEventRow, AuditOutcome, AuditRowId, AuditSelector, DriftEventRow,
        DriftRowId, ParentChain, ProposalId, ProposalRow, Snapshot, SnapshotId, UnixSeconds,
    },
};

use crate::db_errors::sqlx_err;
use crate::lock::LockHandle;

/// Date range filter for snapshot fetch operations.
#[derive(Debug, Clone, Default)]
pub struct SnapshotDateRange {
    /// Lower bound on `created_at_unix_seconds` (inclusive).
    pub since: Option<UnixSeconds>,
    /// Upper bound on `created_at_unix_seconds` (inclusive).
    pub until: Option<UnixSeconds>,
    /// Maximum number of rows to return.  `None` means no cap (use with care
    /// on large tables — prefer setting an explicit limit in production).
    pub max_results: Option<u32>,
}

/// `SQLite`-backed implementation of [`Storage`].
pub struct SqliteStorage {
    pool: SqlitePool,
    _lock_handle: LockHandle,
    #[allow(dead_code)]
    // zd:phase-02 expires:2026-08-01 reason: data_dir kept for future diagnostics / path queries
    _data_dir: PathBuf,
}

impl SqliteStorage {
    /// Open (or create) the database at `<data_dir>/trilithon.db`.
    ///
    /// This method acquires the advisory lock and builds the connection pool
    /// with the required pragmas.  It does **not** run migrations; the caller
    /// must invoke [`crate::migrate::apply_migrations`] after obtaining the
    /// pool (already done in `run.rs`).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the advisory lock cannot be acquired
    /// or when filesystem operations fail.  Returns [`StorageError::Sqlite`]
    /// when the pool cannot be created.
    pub async fn open(data_dir: &Path) -> Result<Self, StorageError> {
        // 1. Acquire the advisory lock first — fail fast if a peer holds it.
        let lock_handle = LockHandle::acquire(data_dir).map_err(|e| StorageError::Io {
            source: std::io::Error::other(e.to_string()),
        })?;

        // 2. Build connection options with required pragmas baked in.
        // Use filename() instead of a formatted URL so paths containing spaces
        // or special characters (e.g. `#`, `?`) are handled correctly.
        let opts = SqliteConnectOptions::new()
            .filename(data_dir.join("trilithon.db"))
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        // 3. Create the pool.
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(opts)
            .await
            .map_err(|e| StorageError::Sqlite {
                kind: SqliteErrorKind::Other(e.to_string()),
            })?;

        Ok(Self {
            pool,
            _lock_handle: lock_handle,
            _data_dir: data_dir.to_owned(),
        })
    }

    /// Return a reference to the underlying connection pool.
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Read `PRAGMA application_id` and verify it matches [`trilithon_core::storage::APPLICATION_ID`].
    ///
    /// Must be called **after** migrations have been applied, because a brand-new
    /// database starts with `application_id = 0` and migration 0005 sets the value.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Sqlite`] with `kind: Other` when the id does not
    /// match, indicating the pool is connected to the wrong database file.
    pub async fn verify_application_id(&self) -> Result<(), StorageError> {
        let raw: i64 = sqlx::query_scalar("PRAGMA application_id")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::Sqlite {
                kind: SqliteErrorKind::Other(e.to_string()),
            })?;
        // PRAGMA application_id returns a signed 32-bit integer stored in the
        // file header.  Our constant is always non-negative; treat negative as
        // a mismatch rather than a conversion error.
        let actual = u32::try_from(raw).unwrap_or(u32::MAX);
        let expected = trilithon_core::storage::APPLICATION_ID;
        if actual != expected {
            return Err(StorageError::Sqlite {
                kind: SqliteErrorKind::Other(format!(
                    "application_id mismatch — wrong database file? \
                     expected {expected:#010x} ({expected}), got {actual:#010x} ({actual})"
                )),
            });
        }
        Ok(())
    }

    /// Fetch all snapshots whose `config_version` equals the given value.
    ///
    /// Returns an empty `Vec` when no matching row exists.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] on database or row-mapping failure.
    pub async fn fetch_by_config_version(
        &self,
        config_version: i64,
    ) -> Result<Vec<Snapshot>, StorageError> {
        let rows = sqlx::query(
            r"
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, canonical_json_version,
                   desired_state_json
            FROM snapshots
            WHERE config_version = ?
            ORDER BY created_at ASC
            ",
        )
        .bind(config_version)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_err)?;

        rows.iter().map(row_to_snapshot).collect()
    }

    /// Fetch all snapshots whose `parent_id` equals the given value.
    ///
    /// Results are ordered by `config_version ASC` to reflect commit-sequence
    /// order for lineage traversal.  Other fetch methods use `created_at ASC`;
    /// the difference is intentional — `config_version` is the natural
    /// ordering for parent–child chains, `created_at` for time-based listing.
    ///
    /// Returns an empty `Vec` when no matching row exists.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] on database or row-mapping failure.
    pub async fn fetch_by_parent_id(
        &self,
        parent_id: &SnapshotId,
    ) -> Result<Vec<Snapshot>, StorageError> {
        let rows = sqlx::query(
            r"
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, canonical_json_version,
                   desired_state_json
            FROM snapshots
            WHERE parent_id = ?
            ORDER BY config_version ASC
            ",
        )
        .bind(&parent_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_err)?;

        rows.iter().map(row_to_snapshot).collect()
    }

    /// Fetch snapshots within a `created_at_unix_seconds` date range.
    ///
    /// Both bounds are optional and inclusive.  Returns an empty `Vec` when no
    /// rows match.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] on database or row-mapping failure.
    pub async fn fetch_by_date_range(
        &self,
        range: &SnapshotDateRange,
    ) -> Result<Vec<Snapshot>, StorageError> {
        // Use fully static query strings — one per combination of since/until
        // present or absent — to avoid any dynamic SQL construction that could
        // become a SQL injection vector if field types change in the future.
        // LIMIT ? is always bound: -1 is the SQLite sentinel for "no limit".
        const SQL_NONE: &str = r"
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, canonical_json_version,
                   desired_state_json
            FROM snapshots
            ORDER BY created_at ASC
            LIMIT ?
        ";
        const SQL_SINCE: &str = r"
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, canonical_json_version,
                   desired_state_json
            FROM snapshots
            WHERE created_at >= ?
            ORDER BY created_at ASC
            LIMIT ?
        ";
        const SQL_UNTIL: &str = r"
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, canonical_json_version,
                   desired_state_json
            FROM snapshots
            WHERE created_at <= ?
            ORDER BY created_at ASC
            LIMIT ?
        ";
        const SQL_BOTH: &str = r"
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, canonical_json_version,
                   desired_state_json
            FROM snapshots
            WHERE created_at >= ? AND created_at <= ?
            ORDER BY created_at ASC
            LIMIT ?
        ";

        // -1 is the SQLite sentinel for "no limit".
        let limit: i64 = range.max_results.map_or(-1, i64::from);

        let rows = match (range.since, range.until) {
            (None, None) => sqlx::query(SQL_NONE)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(sqlx_err)?,
            (Some(since), None) => sqlx::query(SQL_SINCE)
                .bind(since)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(sqlx_err)?,
            (None, Some(until)) => sqlx::query(SQL_UNTIL)
                .bind(until)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(sqlx_err)?,
            (Some(since), Some(until)) => sqlx::query(SQL_BOTH)
                .bind(since)
                .bind(until)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(sqlx_err)?,
        };

        rows.iter().map(row_to_snapshot).collect()
    }

    /// Perform the transactional insert of a validated snapshot.
    ///
    /// Precondition: `validate_snapshot_invariants` has already been called.
    /// Uses `BEGIN IMMEDIATE` to prevent TOCTOU races on the monotonicity check.
    #[allow(clippy::too_many_lines)]
    // zd:phase-05 expires:2026-11-01 reason: atomic transaction cannot be split without
    //   sacrificing readability of the BEGIN IMMEDIATE → dedup → parent → monotonicity → INSERT flow
    async fn insert_snapshot_inner(&self, snapshot: Snapshot) -> Result<SnapshotId, StorageError> {
        let id = snapshot.snapshot_id.0.clone();
        let parent_id = snapshot.parent_id.as_ref().map(|p| p.0.clone());
        // Legacy schema uses actor_kind / actor_id columns.
        // We store a fixed "system" kind and the actor identity string in actor_id.
        let actor_kind = "system";
        // Convert nanoseconds back to milliseconds for legacy created_at_ms column.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        // zd:phase-05 expires:2026-11-01 reason: monotonic_nanos/1_000_000 fits in i64 for sane epoch timestamps
        let created_at_ms = (snapshot.created_at_monotonic_nanos / 1_000_000) as i64;

        // BEGIN IMMEDIATE acquires a write lock immediately, preventing two
        // concurrent callers from both passing the monotonicity check against
        // the same MAX(config_version) — TOCTOU race (ADR-0009).
        let mut conn = self.pool.acquire().await.map_err(sqlx_err)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(sqlx_err)?;

        // 1. Check for deduplication and hash collision.
        let existing_row =
            sqlx::query("SELECT desired_state_json, config_version FROM snapshots WHERE id = ?")
                .bind(&id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(sqlx_err)?;

        if let Some(row) = existing_row {
            let existing_json: String = row.try_get("desired_state_json").map_err(sqlx_err)?;
            let existing_version: i64 = row.try_get("config_version").map_err(sqlx_err)?;
            sqlx::query("ROLLBACK")
                .execute(&mut *conn)
                .await
                .map_err(sqlx_err)?;
            if existing_json != snapshot.desired_state_json {
                return Err(StorageError::SnapshotHashCollision {
                    id: snapshot.snapshot_id,
                });
            }
            // Byte-equal body — verify the caller's config_version is consistent
            // with what was stored to prevent a stale version bypassing monotonicity.
            if existing_version != snapshot.config_version {
                return Err(StorageError::Integrity {
                    detail: format!(
                        "duplicate snapshot {id}: incoming config_version {} does not match \
                         stored {}",
                        snapshot.config_version, existing_version
                    ),
                });
            }
            return Ok(SnapshotId(id));
        }

        // 2. Enforce parent existence when parent_id is Some.
        if let Some(ref pid) = parent_id {
            let parent_exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM snapshots WHERE id = ?)")
                    .bind(pid)
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(sqlx_err)?;
            if !parent_exists {
                sqlx::query("ROLLBACK")
                    .execute(&mut *conn)
                    .await
                    .map_err(sqlx_err)?;
                return Err(StorageError::SnapshotParentNotFound {
                    parent_id: SnapshotId(pid.clone()),
                });
            }
        }

        // 3. Enforce strict monotonic increase of config_version.
        // V1: single local instance — caddy_instance_id is always 'local' (ADR-0009).
        let current_max: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(config_version) FROM snapshots WHERE caddy_instance_id = 'local'",
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(sqlx_err)?;

        if let Some(max_ver) = current_max {
            if snapshot.config_version <= max_ver {
                sqlx::query("ROLLBACK")
                    .execute(&mut *conn)
                    .await
                    .map_err(sqlx_err)?;
                return Err(StorageError::SnapshotVersionNotMonotonic {
                    attempted: snapshot.config_version,
                    current_max: max_ver,
                });
            }
        }

        // 4. INSERT in the same transaction.
        // V1: single local instance — caddy_instance_id is always 'local' (ADR-0009).
        #[allow(clippy::cast_possible_wrap)]
        let created_at_monotonic_ns = snapshot.created_at_monotonic_nanos as i64;

        sqlx::query(
            r"
            INSERT INTO snapshots
                (id, parent_id, caddy_instance_id, actor_kind, actor_id,
                 intent, correlation_id, caddy_version, trilithon_version,
                 created_at, created_at_ms, created_at_monotonic_ns, config_version,
                 canonical_json_version, desired_state_json)
            VALUES (?, ?, 'local', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&id)
        .bind(&parent_id)
        .bind(actor_kind)
        .bind(&snapshot.actor)
        .bind(&snapshot.intent)
        .bind(&snapshot.correlation_id)
        .bind(&snapshot.caddy_version)
        .bind(&snapshot.trilithon_version)
        .bind(snapshot.created_at_unix_seconds)
        .bind(created_at_ms)
        .bind(created_at_monotonic_ns)
        .bind(snapshot.config_version)
        .bind(i64::from(snapshot.canonical_json_version))
        .bind(&snapshot.desired_state_json)
        .execute(&mut *conn)
        .await
        .map_err(sqlx_err)?;

        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .map_err(sqlx_err)?;

        Ok(SnapshotId(id))
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Convert [`ActorKind`] to its lowercase SQL string.
const fn actor_kind_str(k: ActorKind) -> &'static str {
    match k {
        ActorKind::User => "user",
        ActorKind::Token => "token",
        ActorKind::System => "system",
    }
}

/// Parse a lowercase SQL string back to [`ActorKind`].
fn parse_actor_kind(s: &str) -> Result<ActorKind, StorageError> {
    match s {
        "user" => Ok(ActorKind::User),
        "token" => Ok(ActorKind::Token),
        "system" => Ok(ActorKind::System),
        other => Err(StorageError::Integrity {
            detail: format!("unknown actor_kind: {other}"),
        }),
    }
}

/// Convert [`AuditOutcome`] to its lowercase SQL string.
const fn outcome_str(o: AuditOutcome) -> &'static str {
    match o {
        AuditOutcome::Ok => "ok",
        AuditOutcome::Error => "error",
        AuditOutcome::Denied => "denied",
    }
}

/// Parse a lowercase SQL string back to [`AuditOutcome`].
fn parse_outcome(s: &str) -> Result<AuditOutcome, StorageError> {
    match s {
        "ok" => Ok(AuditOutcome::Ok),
        "error" => Ok(AuditOutcome::Error),
        "denied" => Ok(AuditOutcome::Denied),
        other => Err(StorageError::Integrity {
            detail: format!("unknown outcome: {other}"),
        }),
    }
}

// ---------------------------------------------------------------------------
// Row → Snapshot conversion
// ---------------------------------------------------------------------------

fn row_to_snapshot(row: &sqlx::sqlite::SqliteRow) -> Result<Snapshot, StorageError> {
    // Column names use the legacy schema names (pre-T1.2 migration).
    // The Rust field names follow the T1.2 spec; the mapping is documented here:
    //   DB column             → Rust field
    //   id                    → snapshot_id
    //   actor_id              → actor
    //   created_at            → created_at_unix_seconds
    //   created_at_ms         → created_at_monotonic_nanos (stored as ms; converted to ns)
    //   canonical_json_version → canonical_json_version (migration 0005)
    let actor_kind_str: String = row.try_get("actor_kind").map_err(sqlx_err)?;
    parse_actor_kind(&actor_kind_str)?; // validate only; field not stored on Snapshot
    let snapshot_id: String = row.try_get("id").map_err(sqlx_err)?;
    let parent_id: Option<String> = row.try_get("parent_id").map_err(sqlx_err)?;
    let created_at_ms: i64 = row.try_get("created_at_ms").map_err(sqlx_err)?;
    let cjv_raw: i64 = row.try_get("canonical_json_version").map_err(sqlx_err)?;
    let canonical_json_version = u32::try_from(cjv_raw).map_err(|_| StorageError::Integrity {
        detail: format!("canonical_json_version {cjv_raw} is out of u32 range"),
    })?;

    Ok(Snapshot {
        snapshot_id: SnapshotId(snapshot_id),
        parent_id: parent_id.map(SnapshotId),
        config_version: row.try_get("config_version").map_err(sqlx_err)?,
        actor: row.try_get("actor_id").map_err(sqlx_err)?,
        intent: row.try_get("intent").map_err(sqlx_err)?,
        correlation_id: row.try_get("correlation_id").map_err(sqlx_err)?,
        caddy_version: row.try_get("caddy_version").map_err(sqlx_err)?,
        trilithon_version: row.try_get("trilithon_version").map_err(sqlx_err)?,
        created_at_unix_seconds: row.try_get("created_at").map_err(sqlx_err)?,
        // Legacy schema stores milliseconds; convert to nanoseconds for T1.2 field.
        #[allow(clippy::cast_sign_loss)]
        // zd:phase-05 expires:2026-11-01 reason: epoch ms is always non-negative for valid rows
        created_at_monotonic_nanos: (created_at_ms as u64).saturating_mul(1_000_000),
        canonical_json_version,
        desired_state_json: row.try_get("desired_state_json").map_err(sqlx_err)?,
    })
}

// ---------------------------------------------------------------------------
// Audit row conversion (without prev_hash column)
// ---------------------------------------------------------------------------

/// Map a [`SqliteRow`] that does **not** include the `prev_hash` column to an
/// [`AuditEventRow`].  Used inside the hash-chain transaction where we SELECT
/// all columns except `prev_hash` from the previous row.
fn row_to_audit_event_row_no_prev_hash(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<AuditEventRow, StorageError> {
    use trilithon_core::storage::types::AuditRowId;
    let actor_kind_s: String = row.try_get("actor_kind").map_err(sqlx_err)?;
    let actor_kind = parse_actor_kind(&actor_kind_s)?;
    let outcome_s: String = row.try_get("outcome").map_err(sqlx_err)?;
    let outcome = parse_outcome(&outcome_s)?;
    let snapshot_id_s: Option<String> = row.try_get("snapshot_id").map_err(sqlx_err)?;
    let redaction_sites_raw: i64 = row.try_get("redaction_sites").map_err(sqlx_err)?;

    Ok(AuditEventRow {
        id: AuditRowId(row.try_get("id").map_err(sqlx_err)?),
        // prev_hash is intentionally left as the seed value here because this
        // function is only called to build the canonical JSON for hashing the
        // PREVIOUS row, where prev_hash is not part of the hash input.
        prev_hash: trilithon_core::storage::helpers::audit_prev_hash_seed().to_string(),
        caddy_instance_id: row.try_get("caddy_instance_id").map_err(sqlx_err)?,
        correlation_id: row.try_get("correlation_id").map_err(sqlx_err)?,
        occurred_at: row.try_get("occurred_at").map_err(sqlx_err)?,
        occurred_at_ms: row.try_get("occurred_at_ms").map_err(sqlx_err)?,
        actor_kind,
        actor_id: row.try_get("actor_id").map_err(sqlx_err)?,
        kind: row.try_get("kind").map_err(sqlx_err)?,
        target_kind: row.try_get("target_kind").map_err(sqlx_err)?,
        target_id: row.try_get("target_id").map_err(sqlx_err)?,
        snapshot_id: snapshot_id_s.map(SnapshotId),
        redacted_diff_json: row.try_get("redacted_diff_json").map_err(sqlx_err)?,
        redaction_sites: u32::try_from(redaction_sites_raw).unwrap_or(0),
        outcome,
        error_kind: row.try_get("error_kind").map_err(sqlx_err)?,
        notes: row.try_get("notes").map_err(sqlx_err)?,
    })
}

// ---------------------------------------------------------------------------
// Storage trait implementation helpers
// ---------------------------------------------------------------------------

/// Validate snapshot invariants that must hold before any database write.
///
/// 1. `snapshot_id` must equal the SHA-256 hex digest of `desired_state_json`.
/// 2. `intent` must not exceed `INTENT_MAX_BYTES`.
fn validate_snapshot_invariants(snapshot: &Snapshot) -> Result<(), StorageError> {
    let expected = trilithon_core::canonical_json::content_address_bytes(
        snapshot.desired_state_json.as_bytes(),
    );
    if expected != snapshot.snapshot_id.0 {
        return Err(StorageError::Integrity {
            detail: format!(
                "snapshot_id {} does not match SHA-256 of desired_state_json (expected {expected})",
                snapshot.snapshot_id.0
            ),
        });
    }

    if !trilithon_core::storage::types::Snapshot::validate_intent(&snapshot.intent) {
        return Err(StorageError::Integrity {
            detail: format!(
                "intent field exceeds maximum length ({} bytes)",
                snapshot.intent.len()
            ),
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Storage trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Storage for SqliteStorage {
    async fn insert_snapshot(&self, snapshot: Snapshot) -> Result<SnapshotId, StorageError> {
        // Validate invariants before touching the database.
        validate_snapshot_invariants(&snapshot)?;
        // Run the transactional insert logic in a dedicated helper to keep
        // the trait impl method under the too-many-lines lint threshold.
        self.insert_snapshot_inner(snapshot).await
    }

    async fn get_snapshot(&self, id: &SnapshotId) -> Result<Option<Snapshot>, StorageError> {
        let row = sqlx::query(
            r"
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, canonical_json_version,
                   desired_state_json
            FROM snapshots
            WHERE id = ?
            ",
        )
        .bind(&id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err)?;

        row.map(|r| row_to_snapshot(&r)).transpose()
    }

    async fn parent_chain(
        &self,
        leaf: &SnapshotId,
        max_depth: usize,
    ) -> Result<ParentChain, StorageError> {
        // SQLite's WITH RECURSIVE walks up to `max_depth + 1` rows so we can
        // detect truncation.
        let depth_limit = i64::try_from(max_depth + 1).unwrap_or(i64::MAX);

        let rows = sqlx::query(
            r"
            WITH RECURSIVE chain(id, parent_id, caddy_instance_id, actor_kind,
                                  actor_id, intent, correlation_id, caddy_version,
                                  trilithon_version, created_at, created_at_ms,
                                  config_version, canonical_json_version,
                                  desired_state_json, depth) AS (
                SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                       intent, correlation_id, caddy_version, trilithon_version,
                       created_at, created_at_ms, config_version, canonical_json_version,
                       desired_state_json, 0 AS depth
                FROM snapshots WHERE id = ?
                UNION ALL
                SELECT s.id, s.parent_id, s.caddy_instance_id, s.actor_kind, s.actor_id,
                       s.intent, s.correlation_id, s.caddy_version, s.trilithon_version,
                       s.created_at, s.created_at_ms, s.config_version,
                       s.canonical_json_version, s.desired_state_json, c.depth + 1
                FROM snapshots s
                JOIN chain c ON s.id = c.parent_id
                WHERE c.depth < ?
            )
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, canonical_json_version,
                   desired_state_json, depth
            FROM chain
            ORDER BY depth DESC
            ",
        )
        .bind(&leaf.0)
        .bind(depth_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_err)?;

        // If we got `max_depth + 1` rows we hit the limit → truncated.
        let truncated = rows.len() > max_depth;
        let rows_to_keep = if truncated { max_depth } else { rows.len() };

        let mut snapshots: Vec<Snapshot> = Vec::with_capacity(rows_to_keep);
        for r in rows.into_iter().take(rows_to_keep) {
            snapshots.push(row_to_snapshot(&r)?);
        }

        Ok(ParentChain {
            snapshots,
            truncated,
        })
    }

    async fn latest_desired_state(&self) -> Result<Option<Snapshot>, StorageError> {
        let row = sqlx::query(
            r"
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, canonical_json_version,
                   desired_state_json
            FROM snapshots
            ORDER BY config_version DESC
            LIMIT 1
            ",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err)?;

        row.map(|r| row_to_snapshot(&r)).transpose()
    }

    async fn record_audit_event(&self, event: AuditEventRow) -> Result<AuditRowId, StorageError> {
        if !AUDIT_KINDS.contains(&event.kind.as_str()) {
            return Err(StorageError::AuditKindUnknown { kind: event.kind });
        }

        let id = event.id.0.clone();
        let actor_kind = actor_kind_str(event.actor_kind);
        let outcome = outcome_str(event.outcome);
        let snapshot_id = event.snapshot_id.as_ref().map(|s| s.0.clone());

        // Acquire a single connection and use BEGIN IMMEDIATE so the SELECT +
        // INSERT is atomic under a write lock.  prev_hash is always derived
        // from the true last row (tiebreak by id to handle same-ms rows).
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| StorageError::Sqlite {
                kind: SqliteErrorKind::Other(e.to_string()),
            })?;

        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| StorageError::Sqlite {
                kind: SqliteErrorKind::Other(e.to_string()),
            })?;

        // Fetch the last row to chain the hash.
        let prev_hash_result: Result<String, StorageError> = async {
            let last_row = sqlx::query(
                r"
                SELECT id, caddy_instance_id, correlation_id, occurred_at, occurred_at_ms,
                       actor_kind, actor_id, kind, target_kind, target_id,
                       snapshot_id, redacted_diff_json, redaction_sites,
                       outcome, error_kind, notes
                FROM audit_log
                ORDER BY occurred_at DESC, id DESC
                LIMIT 1
                ",
            )
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| StorageError::Sqlite {
                kind: SqliteErrorKind::Other(e.to_string()),
            })?;

            if let Some(row) = last_row {
                let prev_row = row_to_audit_event_row_no_prev_hash(&row)?;
                let canon = canonical_json_for_audit_hash(&prev_row);
                Ok(compute_audit_chain_hash(&canon))
            } else {
                Ok(audit_prev_hash_seed().to_string())
            }
        }
        .await;

        let prev_hash = match prev_hash_result {
            Ok(h) => h,
            Err(e) => {
                // Roll back before propagating the error.
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(e);
            }
        };

        let insert_result = sqlx::query(
            r"
            INSERT INTO audit_log
                (id, prev_hash, caddy_instance_id, correlation_id, occurred_at, occurred_at_ms,
                 actor_kind, actor_id, kind, target_kind, target_id,
                 snapshot_id, redacted_diff_json, redaction_sites,
                 outcome, error_kind, notes)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&id)
        .bind(&prev_hash)
        .bind(&event.caddy_instance_id)
        .bind(&event.correlation_id)
        .bind(event.occurred_at)
        .bind(event.occurred_at_ms)
        .bind(actor_kind)
        .bind(&event.actor_id)
        .bind(&event.kind)
        .bind(&event.target_kind)
        .bind(&event.target_id)
        .bind(&snapshot_id)
        .bind(&event.redacted_diff_json)
        .bind(event.redaction_sites)
        .bind(outcome)
        .bind(&event.error_kind)
        .bind(&event.notes)
        .execute(&mut *conn)
        .await
        .map_err(sqlx_err);

        match insert_result {
            Ok(_) => {}
            Err(e) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(e);
            }
        }

        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .map_err(|e| StorageError::Sqlite {
                kind: SqliteErrorKind::Other(e.to_string()),
            })?;

        Ok(AuditRowId(id))
    }

    async fn tail_audit_log(
        &self,
        selector: AuditSelector,
        limit: u32,
    ) -> Result<Vec<AuditEventRow>, StorageError> {
        // Build a WHERE clause dynamically.  Using a format-string here is
        // safe because all user-supplied values go through bind parameters;
        // only the predicate structure is injected as SQL text.
        let mut conditions: Vec<&'static str> = Vec::new();
        let mut kind_glob_param: Option<String> = None;
        let mut actor_id_param: Option<String> = None;
        let mut correlation_id_param: Option<String> = None;
        let mut since_param: Option<i64> = None;
        let mut until_param: Option<i64> = None;

        if selector.kind_glob.is_some() {
            conditions.push("kind LIKE ? ESCAPE '\\'");
            // SQLite uses % as wildcard; replace trailing * with %.
            // Escape LIKE metacharacters % and _ in the prefix to prevent unintended matches.
            kind_glob_param = selector.kind_glob.map(|g| {
                let prefix = trilithon_core::storage::glob_prefix(&g).map(str::to_owned);
                prefix.map_or(g, |p| {
                    let escaped = p
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_");
                    format!("{escaped}%")
                })
            });
        }
        if selector.actor_id.is_some() {
            conditions.push("actor_id = ?");
            actor_id_param = selector.actor_id;
        }
        if selector.correlation_id.is_some() {
            conditions.push("correlation_id = ?");
            correlation_id_param = selector.correlation_id;
        }
        if selector.since.is_some() {
            conditions.push("occurred_at >= ?");
            since_param = selector.since;
        }
        if selector.until.is_some() {
            conditions.push("occurred_at <= ?");
            until_param = selector.until;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            r"
            SELECT id, prev_hash, caddy_instance_id, correlation_id, occurred_at, occurred_at_ms,
                   actor_kind, actor_id, kind, target_kind, target_id,
                   snapshot_id, redacted_diff_json, redaction_sites,
                   outcome, error_kind, notes
            FROM audit_log
            {where_clause}
            ORDER BY occurred_at DESC
            LIMIT ?
            ",
        );

        let mut query = sqlx::query(&sql);
        if let Some(ref v) = kind_glob_param {
            query = query.bind(v.as_str());
        }
        if let Some(ref v) = actor_id_param {
            query = query.bind(v.as_str());
        }
        if let Some(ref v) = correlation_id_param {
            query = query.bind(v.as_str());
        }
        if let Some(v) = since_param {
            query = query.bind(v);
        }
        if let Some(v) = until_param {
            query = query.bind(v);
        }
        query = query.bind(i64::from(limit));

        let rows = query.fetch_all(&self.pool).await.map_err(sqlx_err)?;

        let mut result: Vec<AuditEventRow> = Vec::with_capacity(rows.len());
        for row in &rows {
            let actor_kind_s: String = row.try_get("actor_kind").map_err(sqlx_err)?;
            let actor_kind = parse_actor_kind(&actor_kind_s)?;
            let outcome_s: String = row.try_get("outcome").map_err(sqlx_err)?;
            let outcome = parse_outcome(&outcome_s)?;
            let snapshot_id_s: Option<String> = row.try_get("snapshot_id").map_err(sqlx_err)?;
            let redaction_sites_raw: i64 = row.try_get("redaction_sites").map_err(sqlx_err)?;

            result.push(AuditEventRow {
                id: AuditRowId(row.try_get("id").map_err(sqlx_err)?),
                prev_hash: row.try_get("prev_hash").map_err(sqlx_err)?,
                caddy_instance_id: row.try_get("caddy_instance_id").map_err(sqlx_err)?,
                correlation_id: row.try_get("correlation_id").map_err(sqlx_err)?,
                occurred_at: row.try_get("occurred_at").map_err(sqlx_err)?,
                occurred_at_ms: row.try_get("occurred_at_ms").map_err(sqlx_err)?,
                actor_kind,
                actor_id: row.try_get("actor_id").map_err(sqlx_err)?,
                kind: row.try_get("kind").map_err(sqlx_err)?,
                target_kind: row.try_get("target_kind").map_err(sqlx_err)?,
                target_id: row.try_get("target_id").map_err(sqlx_err)?,
                snapshot_id: snapshot_id_s.map(SnapshotId),
                redacted_diff_json: row.try_get("redacted_diff_json").map_err(sqlx_err)?,
                redaction_sites: u32::try_from(redaction_sites_raw).unwrap_or(0),
                outcome,
                error_kind: row.try_get("error_kind").map_err(sqlx_err)?,
                notes: row.try_get("notes").map_err(sqlx_err)?,
            });
        }

        Ok(result)
    }

    async fn record_drift_event(&self, _event: DriftEventRow) -> Result<DriftRowId, StorageError> {
        Err(StorageError::Migration {
            version: 0,
            detail: "drift_events table arrives in Phase 8".to_string(),
        })
    }

    async fn latest_drift_event(&self) -> Result<Option<DriftEventRow>, StorageError> {
        Err(StorageError::Migration {
            version: 0,
            detail: "drift_events table arrives in Phase 8".to_string(),
        })
    }

    async fn enqueue_proposal(&self, _proposal: ProposalRow) -> Result<ProposalId, StorageError> {
        Err(StorageError::Migration {
            version: 0,
            detail: "proposals table arrives in Phase 4".to_string(),
        })
    }

    async fn dequeue_proposal(&self) -> Result<Option<ProposalRow>, StorageError> {
        Err(StorageError::Migration {
            version: 0,
            detail: "proposals table arrives in Phase 4".to_string(),
        })
    }

    async fn expire_proposals(&self, _now: UnixSeconds) -> Result<u32, StorageError> {
        Err(StorageError::Migration {
            version: 0,
            detail: "proposals table arrives in Phase 4".to_string(),
        })
    }
}

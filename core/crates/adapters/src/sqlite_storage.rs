//! `SqliteStorage` — the SQLite-backed [`Storage`] adapter.
//!
//! Opens (or creates) `<data_dir>/trilithon.db` via `sqlx`, runs migrations,
//! and applies the required pragmas.  An advisory lock at
//! `<data_dir>/trilithon.lock` prevents two daemons from opening the same
//! database.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

use trilithon_core::storage::{
    audit_vocab::AUDIT_KINDS,
    error::{SqliteErrorKind, StorageError},
    trait_def::Storage,
    types::{
        ActorKind, AuditEventRow, AuditOutcome, AuditRowId, AuditSelector, DriftEventRow,
        DriftRowId, ParentChain, ProposalId, ProposalRow, Snapshot, SnapshotId, UnixSeconds,
    },
};

use crate::lock::LockHandle;

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
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the advisory lock cannot be acquired
    /// or when filesystem operations fail.  Returns [`StorageError::Sqlite`]
    /// when the pool cannot be created or migrations fail.
    pub async fn open(data_dir: &Path) -> Result<Self, StorageError> {
        // 1. Acquire the advisory lock first — fail fast if a peer holds it.
        let lock_handle = LockHandle::acquire(data_dir).map_err(|e| StorageError::Io {
            source: std::io::Error::other(e.to_string()),
        })?;

        // 2. Build connection options with required pragmas baked in.
        let db_url = format!("sqlite://{}/trilithon.db", data_dir.display());
        let opts = SqliteConnectOptions::from_str(&db_url)
            .map_err(|e| StorageError::Sqlite {
                kind: SqliteErrorKind::Other(e.to_string()),
            })?
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

        // 4. Run migrations.
        sqlx::migrate!("./migrations")
            .run(&pool)
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

/// Map a sqlx error to a [`StorageError`].
#[allow(clippy::needless_pass_by_value)]
// reason: `sqlx::Error` is non-Copy; the value is consumed transitively via `to_string`
fn sqlx_err(e: sqlx::Error) -> StorageError {
    StorageError::Sqlite {
        kind: SqliteErrorKind::Other(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Row → Snapshot conversion
// ---------------------------------------------------------------------------

fn row_to_snapshot(row: &sqlx::sqlite::SqliteRow) -> Result<Snapshot, StorageError> {
    let actor_kind_str: String = row.try_get("actor_kind").map_err(sqlx_err)?;
    let actor_kind = parse_actor_kind(&actor_kind_str)?;
    let snapshot_id: String = row.try_get("id").map_err(sqlx_err)?;
    let parent_id: Option<String> = row.try_get("parent_id").map_err(sqlx_err)?;

    Ok(Snapshot {
        id: SnapshotId(snapshot_id),
        parent_id: parent_id.map(SnapshotId),
        caddy_instance_id: row.try_get("caddy_instance_id").map_err(sqlx_err)?,
        actor_kind,
        actor_id: row.try_get("actor_id").map_err(sqlx_err)?,
        intent: row.try_get("intent").map_err(sqlx_err)?,
        correlation_id: row.try_get("correlation_id").map_err(sqlx_err)?,
        caddy_version: row.try_get("caddy_version").map_err(sqlx_err)?,
        trilithon_version: row.try_get("trilithon_version").map_err(sqlx_err)?,
        created_at: row.try_get("created_at").map_err(sqlx_err)?,
        created_at_ms: row.try_get("created_at_ms").map_err(sqlx_err)?,
        config_version: row.try_get("config_version").map_err(sqlx_err)?,
        desired_state_json: row.try_get("desired_state_json").map_err(sqlx_err)?,
    })
}

// ---------------------------------------------------------------------------
// Storage trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Storage for SqliteStorage {
    async fn insert_snapshot(&self, snapshot: Snapshot) -> Result<SnapshotId, StorageError> {
        let id = snapshot.id.0.clone();
        let parent_id = snapshot.parent_id.as_ref().map(|p| p.0.clone());
        let actor_kind = actor_kind_str(snapshot.actor_kind);

        let rows_affected = sqlx::query(
            r"
            INSERT OR IGNORE INTO snapshots
                (id, parent_id, caddy_instance_id, actor_kind, actor_id,
                 intent, correlation_id, caddy_version, trilithon_version,
                 created_at, created_at_ms, config_version, desired_state_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&id)
        .bind(&parent_id)
        .bind(&snapshot.caddy_instance_id)
        .bind(actor_kind)
        .bind(&snapshot.actor_id)
        .bind(&snapshot.intent)
        .bind(&snapshot.correlation_id)
        .bind(&snapshot.caddy_version)
        .bind(&snapshot.trilithon_version)
        .bind(snapshot.created_at)
        .bind(snapshot.created_at_ms)
        .bind(snapshot.config_version)
        .bind(&snapshot.desired_state_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err)?
        .rows_affected();

        if rows_affected == 0 {
            // Row already exists — check whether it is an exact duplicate.
            let existing_row = sqlx::query("SELECT desired_state_json FROM snapshots WHERE id = ?")
                .bind(&id)
                .fetch_one(&self.pool)
                .await
                .map_err(sqlx_err)?;

            let existing_json: String = existing_row
                .try_get("desired_state_json")
                .map_err(sqlx_err)?;

            if existing_json == snapshot.desired_state_json {
                // Idempotent — same body, return the existing id.
                return Ok(SnapshotId(id));
            }

            return Err(StorageError::SnapshotDuplicate { id: snapshot.id });
        }

        Ok(SnapshotId(id))
    }

    async fn get_snapshot(&self, id: &SnapshotId) -> Result<Option<Snapshot>, StorageError> {
        let row = sqlx::query(
            r"
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, desired_state_json
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
                                  config_version, desired_state_json, depth) AS (
                SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                       intent, correlation_id, caddy_version, trilithon_version,
                       created_at, created_at_ms, config_version, desired_state_json,
                       0 AS depth
                FROM snapshots WHERE id = ?
                UNION ALL
                SELECT s.id, s.parent_id, s.caddy_instance_id, s.actor_kind, s.actor_id,
                       s.intent, s.correlation_id, s.caddy_version, s.trilithon_version,
                       s.created_at, s.created_at_ms, s.config_version, s.desired_state_json,
                       c.depth + 1
                FROM snapshots s
                JOIN chain c ON s.id = c.parent_id
                WHERE c.depth < ?
            )
            SELECT id, parent_id, caddy_instance_id, actor_kind, actor_id,
                   intent, correlation_id, caddy_version, trilithon_version,
                   created_at, created_at_ms, config_version, desired_state_json, depth
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
                   created_at, created_at_ms, config_version, desired_state_json
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

        sqlx::query(
            r"
            INSERT INTO audit_log
                (id, caddy_instance_id, correlation_id, occurred_at, occurred_at_ms,
                 actor_kind, actor_id, kind, target_kind, target_id,
                 snapshot_id, redacted_diff_json, redaction_sites,
                 outcome, error_kind, notes)
            VALUES (?, 'local', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&id)
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
        .execute(&self.pool)
        .await
        .map_err(sqlx_err)?;

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
            conditions.push("kind LIKE ?");
            // SQLite uses % as wildcard; replace trailing * with %.
            kind_glob_param = selector.kind_glob.map(|g| {
                if g.ends_with('*') {
                    format!("{}%", &g[..g.len() - 1])
                } else {
                    g
                }
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
            SELECT id, correlation_id, occurred_at, occurred_at_ms,
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

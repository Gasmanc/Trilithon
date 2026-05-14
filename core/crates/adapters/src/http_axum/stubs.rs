//! Noop/stub implementations of auth traits for tests and pre-wired CLI.
//!
//! These stubs return errors for every operation.  They exist so that tests
//! that only exercise health or `OpenAPI` routes can construct an [`AppState`]
//! without a real database.  Auth-specific tests use real `SQLite` stores.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use async_trait::async_trait;
use trilithon_core::reconciler::{Applier, ApplyError, ApplyOutcome, ValidationReport};
use trilithon_core::storage::{
    StorageError,
    trait_def::Storage,
    types::{
        AuditEventRow, AuditRowId, AuditSelector, DriftEventRow, DriftResolution, DriftRowId,
        ParentChain, ProposalId, ProposalRow, Snapshot, SnapshotId, UnixSeconds,
    },
};

// ── NoopStorage ───────────────────────────────────────────────────────────────

/// A [`Storage`] implementation that returns errors or empty results.
///
/// Used by [`make_test_app_state`] for tests that only exercise non-storage
/// paths (health, `OpenAPI`, bind-rejection).
pub struct NoopStorage;

#[async_trait]
impl Storage for NoopStorage {
    async fn insert_snapshot(&self, _s: Snapshot) -> Result<SnapshotId, StorageError> {
        Err(StorageError::Integrity {
            detail: "noop".to_owned(),
        })
    }
    async fn get_snapshot(&self, _id: &SnapshotId) -> Result<Option<Snapshot>, StorageError> {
        Ok(None)
    }
    async fn parent_chain(
        &self,
        _leaf: &SnapshotId,
        _max: usize,
    ) -> Result<ParentChain, StorageError> {
        Ok(ParentChain {
            snapshots: vec![],
            truncated: false,
        })
    }
    async fn latest_desired_state(&self) -> Result<Option<Snapshot>, StorageError> {
        Ok(None)
    }
    async fn list_snapshots(
        &self,
        _limit: u32,
        _cursor_before_version: Option<i64>,
    ) -> Result<Vec<Snapshot>, StorageError> {
        Ok(vec![])
    }
    async fn record_audit_event(&self, _e: AuditEventRow) -> Result<AuditRowId, StorageError> {
        Err(StorageError::Integrity {
            detail: "noop".to_owned(),
        })
    }
    async fn tail_audit_log(
        &self,
        _s: AuditSelector,
        _limit: u32,
    ) -> Result<Vec<AuditEventRow>, StorageError> {
        Ok(vec![])
    }
    async fn record_drift_event(&self, _e: DriftEventRow) -> Result<DriftRowId, StorageError> {
        Err(StorageError::Integrity {
            detail: "noop".to_owned(),
        })
    }
    async fn latest_drift_event(&self) -> Result<Option<DriftEventRow>, StorageError> {
        Ok(None)
    }
    async fn latest_unresolved_drift_event(&self) -> Result<Option<DriftEventRow>, StorageError> {
        Ok(None)
    }
    async fn resolve_drift_event(
        &self,
        _c: &str,
        _r: DriftResolution,
        _at: UnixSeconds,
    ) -> Result<(), StorageError> {
        Ok(())
    }
    async fn enqueue_proposal(&self, _p: ProposalRow) -> Result<ProposalId, StorageError> {
        Err(StorageError::Integrity {
            detail: "noop".to_owned(),
        })
    }
    async fn dequeue_proposal(&self) -> Result<Option<ProposalRow>, StorageError> {
        Ok(None)
    }
    async fn expire_proposals(&self, _now: UnixSeconds) -> Result<u32, StorageError> {
        Ok(0)
    }
    async fn current_config_version(&self, _id: &str) -> Result<i64, StorageError> {
        Ok(0)
    }
    async fn cas_advance_config_version(
        &self,
        _id: &str,
        _expected: i64,
        _new: &SnapshotId,
    ) -> Result<i64, StorageError> {
        Err(StorageError::Integrity {
            detail: "noop".to_owned(),
        })
    }
}

// ── NoopApplier ───────────────────────────────────────────────────────────────

/// An [`Applier`] that always returns an error.
///
/// Suitable for tests that do not exercise the mutation/apply paths.
pub struct NoopApplier;

#[async_trait]
impl Applier for NoopApplier {
    async fn apply(
        &self,
        _snapshot: &trilithon_core::storage::types::Snapshot,
        _expected_version: i64,
    ) -> Result<ApplyOutcome, ApplyError> {
        Err(ApplyError::Storage("noop".to_owned()))
    }

    async fn validate(
        &self,
        _snapshot: &trilithon_core::storage::types::Snapshot,
    ) -> Result<ValidationReport, ApplyError> {
        Ok(ValidationReport::default())
    }

    async fn rollback(&self, _target: &SnapshotId) -> Result<ApplyOutcome, ApplyError> {
        Err(ApplyError::Storage("noop".to_owned()))
    }
}

use crate::audit_writer::AuditWriter;
use crate::auth::rate_limit::LoginRateLimiter;
use crate::auth::sessions::{Session, SessionError, SessionStore};
use crate::auth::users::{User, UserRole, UserStore, UserStoreError};
use crate::http_axum::AppState;

// ── NoopSessionStore ──────────────────────────────────────────────────────────

/// A [`SessionStore`] that always returns an error.
///
/// Suitable for tests that don't exercise auth endpoints.
pub struct NoopSessionStore;

#[async_trait]
impl SessionStore for NoopSessionStore {
    async fn create(
        &self,
        _user_id: &str,
        _ttl_seconds: u64,
        _ua: Option<String>,
        _ip: Option<String>,
    ) -> Result<Session, SessionError> {
        Err(SessionError::Db(sqlx::Error::RowNotFound))
    }

    async fn touch(&self, _session_id: &str) -> Result<Option<Session>, SessionError> {
        Ok(None)
    }

    async fn revoke(&self, _session_id: &str) -> Result<(), SessionError> {
        Ok(())
    }

    async fn revoke_all_for_user(&self, _user_id: &str) -> Result<u32, SessionError> {
        Ok(0)
    }
}

// ── NoopUserStore ─────────────────────────────────────────────────────────────

/// A [`UserStore`] that always returns "not found".
///
/// Suitable for tests that don't exercise auth endpoints.
pub struct NoopUserStore;

#[async_trait]
impl UserStore for NoopUserStore {
    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<(User, String)>, UserStoreError> {
        Err(UserStoreError::NotFound(username.to_owned()))
    }

    async fn find_user_by_id(
        &self,
        user_id: &str,
    ) -> Result<Option<(User, String)>, UserStoreError> {
        Err(UserStoreError::NotFound(user_id.to_owned()))
    }

    async fn create_user(
        &self,
        username: &str,
        _password: &str,
        _role: UserRole,
    ) -> Result<User, UserStoreError> {
        Err(UserStoreError::NotFound(username.to_owned()))
    }

    async fn update_password(
        &self,
        user_id: &str,
        _new_password: &str,
    ) -> Result<(), UserStoreError> {
        Err(UserStoreError::NotFound(user_id.to_owned()))
    }

    async fn set_must_change_pw(&self, user_id: &str, _value: bool) -> Result<(), UserStoreError> {
        Err(UserStoreError::NotFound(user_id.to_owned()))
    }

    async fn user_count(&self) -> Result<u64, UserStoreError> {
        Ok(0)
    }
}

// ── Convenience constructor ───────────────────────────────────────────────────

/// Build an [`AppState`] suitable for tests that only exercise non-auth routes.
///
/// The session store, user store, and audit writer are noop/in-memory stubs.
/// Auth endpoint tests should construct [`AppState`] directly with real stores.
pub fn make_test_app_state(
    apply_in_flight: Arc<AtomicBool>,
    ready_since_unix_ms: Arc<AtomicU64>,
) -> Arc<AppState> {
    use trilithon_core::{clock::SystemClock, diff::DefaultDiffEngine, schema::SchemaRegistry};

    use crate::sha256_hasher::Sha256AuditHasher;

    let storage: Arc<dyn trilithon_core::storage::trait_def::Storage> = Arc::new(NoopStorage);
    let schema_registry = Arc::new(SchemaRegistry::with_tier1_secrets());
    let audit_writer = Arc::new(AuditWriter::new_with_arcs(
        Arc::clone(&storage),
        Arc::new(SystemClock),
        Arc::clone(&schema_registry),
        Arc::new(Sha256AuditHasher),
    ));

    Arc::new(AppState {
        apply_in_flight,
        ready_since_unix_ms,
        rate_limiter: Arc::new(LoginRateLimiter::new()),
        session_store: Arc::new(NoopSessionStore),
        user_store: Arc::new(NoopUserStore),
        audit_writer,
        session_cookie_name: "trilithon_session".to_owned(),
        session_ttl_seconds: 12 * 3600,
        token_pool: None,
        applier: Arc::new(NoopApplier),
        storage,
        diff_engine: Arc::new(DefaultDiffEngine),
        schema_registry,
        hasher: Arc::new(Sha256AuditHasher),
    })
}

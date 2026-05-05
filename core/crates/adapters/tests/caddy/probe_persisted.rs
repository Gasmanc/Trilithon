//! Integration test: `run_initial_probe` writes a `capability_probe_results`
//! row, and repeated probes keep exactly one `is_current = 1` row per
//! `caddy_instance_id`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: integration test — panics, unimplemented, and unwrap are the correct failure mode here

use std::collections::BTreeSet;
use std::str::FromStr as _;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::Row as _;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use trilithon_adapters::{
    caddy::{cache::CapabilityCache, capability_store::CapabilityStore, probe::run_initial_probe},
    migrate::apply_migrations,
};
use trilithon_core::caddy::{
    client::CaddyClient,
    error::CaddyError,
    types::{
        CaddyConfig, CaddyJsonPointer, HealthState, JsonPatch, LoadedModules, TlsCertificate,
        UpstreamHealth,
    },
};

// ---------------------------------------------------------------------------
// Test double
// ---------------------------------------------------------------------------

struct FixedModulesClient {
    version: &'static str,
}

#[async_trait]
impl CaddyClient for FixedModulesClient {
    async fn load_config(&self, _body: CaddyConfig) -> Result<(), CaddyError> {
        unimplemented!()
    }

    async fn patch_config(
        &self,
        _path: CaddyJsonPointer,
        _patch: JsonPatch,
    ) -> Result<(), CaddyError> {
        unimplemented!()
    }

    async fn put_config(
        &self,
        _path: CaddyJsonPointer,
        _value: serde_json::Value,
    ) -> Result<(), CaddyError> {
        unimplemented!()
    }

    async fn get_running_config(&self) -> Result<CaddyConfig, CaddyError> {
        unimplemented!()
    }

    async fn get_loaded_modules(&self) -> Result<LoadedModules, CaddyError> {
        Ok(LoadedModules {
            modules: BTreeSet::from(["http.handlers.reverse_proxy".to_owned()]),
            caddy_version: self.version.to_owned(),
        })
    }

    async fn get_upstream_health(&self) -> Result<Vec<UpstreamHealth>, CaddyError> {
        unimplemented!()
    }

    async fn get_certificates(&self) -> Result<Vec<TlsCertificate>, CaddyError> {
        unimplemented!()
    }

    async fn health_check(&self) -> Result<HealthState, CaddyError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

async fn make_pool() -> sqlx::SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite://:memory:")
        .unwrap()
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new().connect_with(opts).await.unwrap();
    apply_migrations(&pool).await.unwrap();
    pool
}

async fn insert_instance(pool: &sqlx::SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO caddy_instances \
         (id, display_name, transport, address, created_at, ownership_token) \
         VALUES (?, 'Test', 'unix', '/tmp/test.sock', 0, 'tok')",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// First probe inserts one row with `is_current = 1`.
/// Second probe demotes the first and inserts a second; total rows = 2,
/// current rows = 1.
#[tokio::test]
async fn probe_writes_current_row() {
    let pool = make_pool().await;
    insert_instance(&pool, "inst-1").await;

    let cache = Arc::new(CapabilityCache::default());
    let store = CapabilityStore::new(pool.clone());
    let client = FixedModulesClient { version: "v2.8.4" };

    // First probe.
    run_initial_probe(&client, Arc::clone(&cache), &store, "inst-1")
        .await
        .expect("first probe should succeed");

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM capability_probe_results WHERE caddy_instance_id = 'inst-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total, 1, "expected 1 row after first probe");

    let current: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM capability_probe_results WHERE caddy_instance_id = 'inst-1' AND is_current = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        current, 1,
        "expected exactly 1 is_current row after first probe"
    );

    // Second probe.
    run_initial_probe(&client, Arc::clone(&cache), &store, "inst-1")
        .await
        .expect("second probe should succeed");

    let total2: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM capability_probe_results WHERE caddy_instance_id = 'inst-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total2, 2, "expected 2 rows after second probe");

    let current2: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM capability_probe_results WHERE caddy_instance_id = 'inst-1' AND is_current = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        current2, 1,
        "expected exactly 1 is_current row after second probe"
    );

    // Verify the most-recent row is the one marked current.
    let current_row = sqlx::query(
        "SELECT caddy_version FROM capability_probe_results \
         WHERE caddy_instance_id = 'inst-1' AND is_current = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let version: String = current_row.try_get("caddy_version").unwrap();
    assert_eq!(version, "v2.8.4");
}

//! E2E timing test: `run_initial_probe` must complete within 1 000 ms against
//! a real Caddy 2.8 process.
//!
//! Gated on `TRILITHON_E2E_CADDY=1`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: E2E test — panics and unwrap are the correct failure mode here

use std::str::FromStr as _;
use std::sync::Arc;
use std::time::Instant;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use trilithon_adapters::{
    caddy::{
        cache::CapabilityCache, capability_store::CapabilityStore, hyper_client::HyperCaddyClient,
        probe::run_initial_probe,
    },
    migrate::apply_migrations,
};
use trilithon_core::config::CaddyEndpoint;

async fn make_pool() -> sqlx::SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite://:memory:")
        .unwrap()
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new().connect_with(opts).await.unwrap();
    apply_migrations(&pool).await.unwrap();
    pool
}

/// Run `run_initial_probe` against a live Caddy and assert it completes within
/// 1 000 ms.
///
/// This test is gated on `TRILITHON_E2E_CADDY=1` to avoid running in CI
/// environments that do not have Caddy installed.
#[tokio::test]
async fn probe_within_one_second() {
    if std::env::var("TRILITHON_E2E_CADDY").as_deref() != Ok("1") {
        return;
    }

    // Expect a Caddy admin socket at the path supplied by the environment, or
    // a default test socket path.
    let socket_path = std::env::var("TRILITHON_E2E_CADDY_SOCKET")
        .unwrap_or_else(|_| "/tmp/caddy-e2e-test.sock".to_owned());

    let endpoint = CaddyEndpoint::Unix {
        path: socket_path.into(),
    };

    let client = HyperCaddyClient::from_config(
        &endpoint,
        std::time::Duration::from_secs(5),
        std::time::Duration::from_secs(5),
    )
    .expect("client construction should succeed");

    let pool = make_pool().await;

    // Insert a caddy_instances row so the FK constraint is satisfied.
    sqlx::query(
        "INSERT INTO caddy_instances \
         (id, display_name, transport, address, created_at, ownership_token) \
         VALUES ('e2e-instance', 'E2E', 'unix', '/tmp/caddy-e2e-test.sock', 0, 'tok')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let cache = Arc::new(CapabilityCache::default());
    let store = CapabilityStore::new(pool);

    let start = Instant::now();
    run_initial_probe(&client, cache, &store, "e2e-instance")
        .await
        .expect("probe should succeed");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 1_000,
        "probe took {}ms, expected < 1000ms",
        elapsed.as_millis()
    );
}

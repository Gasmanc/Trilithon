//! Startup capability probe runner.
//!
//! [`run_initial_probe`] calls the Caddy admin API once, caches the result
//! in a [`CapabilityCache`], and persists a row with `is_current = 1` in the
//! `capability_probe_results` table.

use std::sync::Arc;

use trilithon_core::caddy::{
    capabilities::CaddyCapabilities, client::CaddyClient, error::CaddyError,
};
use trilithon_core::storage::error::StorageError;

use crate::caddy::{cache::CapabilityCache, capability_store::CapabilityStore};

/// Errors that can occur during the initial capability probe.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// A Caddy admin API call failed.
    #[error("caddy error during probe: {source}")]
    Caddy {
        /// The underlying Caddy error.
        #[from]
        source: CaddyError,
    },
    /// A database persistence operation failed.
    #[error("storage error during probe: {source}")]
    Storage {
        /// The underlying storage error.
        #[from]
        source: StorageError,
    },
}

/// Run the initial capability probe.
///
/// 1. Calls [`CaddyClient::get_loaded_modules`] to retrieve the live module
///    list and Caddy version.
/// 2. Constructs a [`CaddyCapabilities`] stamped with the current UTC time.
/// 3. Writes the value into `cache`.
/// 4. Persists the value via `persistence`, demoting any previously current
///    row for `instance_id`.
/// 5. Emits a `tracing::info!` event with the field key
///    `"caddy.capability-probe.completed"`.
///
/// # Errors
///
/// Returns [`ProbeError::Caddy`] if the admin API call fails, or
/// [`ProbeError::Storage`] if persistence fails.
pub async fn run_initial_probe(
    client: &dyn CaddyClient,
    cache: Arc<CapabilityCache>,
    persistence: &CapabilityStore,
    instance_id: &str,
) -> Result<CaddyCapabilities, ProbeError> {
    let modules = client.get_loaded_modules().await?;

    let now = time::OffsetDateTime::now_utc().unix_timestamp();

    let caps = CaddyCapabilities {
        loaded_modules: modules.modules,
        caddy_version: modules.caddy_version,
        probed_at: now,
    };

    cache.replace(caps.clone());
    persistence.record_current(instance_id, &caps).await?;

    let correlation_id = ulid::Ulid::new().to_string();
    tracing::info!(
        correlation_id = %correlation_id,
        caddy_version = %caps.caddy_version,
        module_count = caps.loaded_modules.len(),
        "caddy.capability-probe.completed",
    );

    Ok(caps)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: test-only code; panics and unimplemented are the correct failure mode in tests
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use async_trait::async_trait;

    use trilithon_core::caddy::{
        client::CaddyClient,
        error::CaddyError,
        types::{
            CaddyConfig, CaddyJsonPointer, HealthState, JsonPatch, LoadedModules, TlsCertificate,
            UpstreamHealth,
        },
    };

    use super::*;

    // -----------------------------------------------------------------------
    // Test double
    // -----------------------------------------------------------------------

    struct CaddyClientDouble {
        modules: LoadedModules,
    }

    #[async_trait]
    impl CaddyClient for CaddyClientDouble {
        async fn load_config(&self, _body: CaddyConfig) -> Result<(), CaddyError> {
            unimplemented!("not needed in this test")
        }

        async fn patch_config(
            &self,
            _path: CaddyJsonPointer,
            _patch: JsonPatch,
        ) -> Result<(), CaddyError> {
            unimplemented!("not needed in this test")
        }

        async fn get_running_config(&self) -> Result<CaddyConfig, CaddyError> {
            unimplemented!("not needed in this test")
        }

        async fn get_loaded_modules(&self) -> Result<LoadedModules, CaddyError> {
            Ok(self.modules.clone())
        }

        async fn get_upstream_health(&self) -> Result<Vec<UpstreamHealth>, CaddyError> {
            unimplemented!("not needed in this test")
        }

        async fn get_certificates(&self) -> Result<Vec<TlsCertificate>, CaddyError> {
            unimplemented!("not needed in this test")
        }

        async fn health_check(&self) -> Result<HealthState, CaddyError> {
            unimplemented!("not needed in this test")
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn fixed_modules() -> LoadedModules {
        LoadedModules {
            modules: BTreeSet::from([
                "http.handlers.reverse_proxy".to_owned(),
                "http.handlers.static_response".to_owned(),
            ]),
            caddy_version: "v2.8.4".to_owned(),
        }
    }

    async fn make_store() -> (CapabilityStore, sqlx::SqlitePool) {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use std::str::FromStr as _;
        let opts = SqliteConnectOptions::from_str("sqlite://:memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new().connect_with(opts).await.unwrap();
        crate::migrate::apply_migrations(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO caddy_instances \
             (id, display_name, transport, address, created_at, ownership_token) \
             VALUES ('test-instance', 'Test', 'unix', '/tmp/test.sock', 0, 'tok')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = CapabilityStore::new(pool.clone());
        (store, pool)
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// `run_initial_probe` must populate the cache and emit the
    /// `caddy.capability-probe.completed` event.  The tracing event name is
    /// verified by a custom [`tracing::Subscriber`] layer installed for the
    /// duration of the test via a dedicated single-thread runtime.  The cache
    /// snapshot is checked directly.
    #[test]
    fn probe_emits_event_and_caches() {
        use std::sync::Mutex;

        use tracing::subscriber::with_default;
        use tracing_subscriber::layer::SubscriberExt as _;

        use crate::test_support::EventCollector;

        let events: Arc<Mutex<Vec<String>>> = Arc::default();
        let collector = EventCollector {
            events: Arc::clone(&events),
        };
        let subscriber = tracing_subscriber::registry().with(collector);

        // Build a fresh single-thread runtime so that `with_default` wraps
        // the entire async execution without nesting inside another runtime.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let (store, _pool, cache) = rt.block_on(async {
            let (store, pool) = make_store().await;
            let cache = Arc::new(CapabilityCache::default());
            (store, pool, cache)
        });

        let caps = with_default(subscriber, || {
            rt.block_on(async {
                let client = CaddyClientDouble {
                    modules: fixed_modules(),
                };
                run_initial_probe(&client, Arc::clone(&cache), &store, "test-instance").await
            })
        })
        .expect("probe should succeed");

        // Cache must be populated.
        let snapshot = cache
            .snapshot()
            .expect("cache must be non-empty after probe");
        assert_eq!(snapshot.caddy_version, caps.caddy_version);
        assert_eq!(snapshot.loaded_modules, caps.loaded_modules);

        // The returned caps must match the fixed double's data.
        assert_eq!(caps.caddy_version, "v2.8.4");
        assert_eq!(caps.loaded_modules.len(), 2);

        // The `caddy.capability-probe.completed` event must have been emitted.
        let emitted = events.lock().unwrap().clone();
        assert!(
            emitted
                .iter()
                .any(|n| n == "caddy.capability-probe.completed"),
            "expected caddy.capability-probe.completed in emitted events; got: {emitted:?}",
        );
    }
}

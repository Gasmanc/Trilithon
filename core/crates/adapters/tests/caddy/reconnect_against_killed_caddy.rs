//! Reconnect logic test: scripted client goes dead for one health cycle then
//! recovers. Verifies `caddy.connected` and `caddy.capability-probe.completed`
//! are emitted after reconnect.
//!
//! Uses a call-count-based `ScriptedClient` and a 50 ms health interval so
//! the test completes in well under one second of real wall-clock time.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::collections::BTreeSet;
use std::str::FromStr as _;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use async_trait::async_trait;
use tracing_subscriber::layer::SubscriberExt as _;
use trilithon_adapters::{
    caddy::{
        cache::CapabilityCache,
        capability_store::CapabilityStore,
        reconnect::{ShutdownObserver, reconnect_loop},
    },
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
// Event capture
// ---------------------------------------------------------------------------

struct EventCaptureLayer {
    events: Arc<Mutex<Vec<String>>>,
}

struct MessageVisitor<'a> {
    message: &'a mut Option<String>,
}

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.message = Some(format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            *self.message = Some(value.to_owned());
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for EventCaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut msg: Option<String> = None;
        event.record(&mut MessageVisitor { message: &mut msg });
        if let Some(m) = msg {
            self.events.lock().unwrap().push(m);
        }
    }
}

// ---------------------------------------------------------------------------
// Test double — scripted by call count, not wall-clock time
// ---------------------------------------------------------------------------

/// Returns `Unreachable` for the first `dead_calls` health-check invocations,
/// then `Reachable` for all subsequent calls.
struct CallCountedClient {
    call_count: Arc<AtomicUsize>,
    dead_calls: usize,
    modules: LoadedModules,
}

#[async_trait]
impl CaddyClient for CallCountedClient {
    async fn health_check(&self) -> Result<HealthState, CaddyError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        if n < self.dead_calls {
            Ok(HealthState::Unreachable)
        } else {
            Ok(HealthState::Reachable)
        }
    }

    async fn get_loaded_modules(&self) -> Result<LoadedModules, CaddyError> {
        Ok(self.modules.clone())
    }

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

    async fn get_upstream_health(&self) -> Result<Vec<UpstreamHealth>, CaddyError> {
        unimplemented!()
    }

    async fn get_certificates(&self) -> Result<Vec<TlsCertificate>, CaddyError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Shutdown observer — times out after a fixed wall-clock duration
// ---------------------------------------------------------------------------

struct TimedShutdown {
    deadline: std::time::Instant,
}

impl ShutdownObserver for TimedShutdown {
    fn changed(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let remaining = self
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        Box::pin(async move {
            tokio::time::sleep(remaining).await;
        })
    }

    fn is_shutting_down(&self) -> bool {
        std::time::Instant::now() >= self.deadline
    }
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// The reconnect loop must emit `caddy.connected` and
/// `caddy.capability-probe.completed` after a one-cycle dead window.
///
/// Uses a 50 ms health interval and a call-count-based client so the test
/// completes in well under one second without any clock manipulation.
///
/// Schedule (approximate wall-clock):
///
/// - t=0 ms: loop starts in `Reachable` state, sleeps `health_interval` (50 ms)
/// - t≈50 ms: first `health_check` → call #0 → `Unreachable` → caddy.disconnected;
///            backoff doubles from 250 ms to 500 ms
/// - t≈550 ms: second `health_check` → call #1 → `Reachable` → caddy.connected
///             + probe → caddy.capability-probe.completed
/// - t=1500 ms: `TimedShutdown` fires → loop exits
#[tokio::test]
async fn observes_fresh_probe_after_reconnect() {
    let events: Arc<Mutex<Vec<String>>> = Arc::default();
    let layer = EventCaptureLayer {
        events: Arc::clone(&events),
    };
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    let pool = {
        let opts = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite://:memory:")
            .unwrap()
            .create_if_missing(true);
        sqlx::sqlite::SqlitePoolOptions::new()
            .connect_with(opts)
            .await
            .unwrap()
    };
    apply_migrations(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO caddy_instances \
         (id, display_name, transport, address, created_at, ownership_token) \
         VALUES ('test-inst', 'Test', 'unix', '/tmp/test.sock', 0, 'tok')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let store = CapabilityStore::new(pool);
    let cache = Arc::new(CapabilityCache::default());
    let client = Arc::new(CallCountedClient {
        call_count: Arc::new(AtomicUsize::new(0)),
        dead_calls: 1,
        modules: LoadedModules {
            modules: BTreeSet::from(["http.handlers.reverse_proxy".to_owned()]),
            caddy_version: "v2.8.4".to_owned(),
        },
    });

    // 50 ms health interval keeps the test fast; 1500 ms shutdown gives the
    // loop enough time to complete the disconnect + reconnect + probe cycle.
    let health_interval = Duration::from_millis(50);
    let shutdown = TimedShutdown {
        deadline: std::time::Instant::now() + Duration::from_millis(1500),
    };

    reconnect_loop(
        Arc::clone(&client) as Arc<dyn CaddyClient>,
        Arc::clone(&cache),
        store,
        "test-inst".to_owned(),
        shutdown,
        health_interval,
    )
    .await;

    let captured = events.lock().unwrap().clone();
    assert!(
        captured.iter().any(|m| m == "caddy.disconnected"),
        "expected caddy.disconnected; got: {captured:?}",
    );
    assert!(
        captured.iter().any(|m| m == "caddy.connected"),
        "expected caddy.connected; got: {captured:?}",
    );
    assert!(
        captured
            .iter()
            .any(|m| m == "caddy.capability-probe.completed"),
        "expected caddy.capability-probe.completed; got: {captured:?}",
    );

    // Backoff after first disconnect: INITIAL_BACKOFF (250 ms) doubled to 500 ms.
    // Verify that the probe fires before the shutdown deadline.
    let probe_idx = captured
        .iter()
        .position(|m| m == "caddy.capability-probe.completed")
        .unwrap();
    let connect_idx = captured
        .iter()
        .position(|m| m == "caddy.connected")
        .unwrap();
    assert!(
        connect_idx < probe_idx,
        "caddy.connected must precede caddy.capability-probe.completed",
    );
}

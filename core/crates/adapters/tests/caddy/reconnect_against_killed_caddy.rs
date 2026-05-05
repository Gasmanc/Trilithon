//! E2E test: kill Caddy mid-loop, restart after 5 s, assert a fresh
//! `caddy.capability-probe.completed` event appears within 35 s of restart.
//!
//! Gated behind `TRILITHON_E2E_CADDY=1`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: E2E test — panics are the correct failure mode in tests

use std::collections::BTreeSet;
use std::str::FromStr as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
// Test double — simulates Caddy being killed then restarted
// ---------------------------------------------------------------------------

struct EventCaptureLayer {
    events: Arc<Mutex<Vec<(String, Instant)>>>,
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
            self.events.lock().unwrap().push((m, Instant::now()));
        }
    }
}

/// Simulates a Caddy instance that is alive, then dead for a window, then alive again.
struct ScriptedClient {
    /// Timestamps (from `Instant::now()` at test start) during which the
    /// client should appear unreachable.
    dead_window: (Duration, Duration),
    start: Instant,
    modules: LoadedModules,
}

#[async_trait]
impl CaddyClient for ScriptedClient {
    async fn health_check(&self) -> Result<HealthState, CaddyError> {
        let elapsed = self.start.elapsed();
        if elapsed >= self.dead_window.0 && elapsed < self.dead_window.1 {
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
// Shutdown observer
// ---------------------------------------------------------------------------

struct TimedShutdown {
    deadline: Instant,
}

impl ShutdownObserver for TimedShutdown {
    fn changed(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        Box::pin(async move {
            tokio::time::sleep(remaining).await;
        })
    }

    fn is_shutting_down(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// Kill the scripted Caddy at t=2 s, restart at t=7 s (5 s dead window).
/// Assert a fresh `caddy.capability-probe.completed` event arrives within
/// 35 s of restart (i.e. t < 42 s overall).
///
/// Gated: only runs when `TRILITHON_E2E_CADDY=1`.
#[tokio::test]
async fn observes_fresh_probe_within_35s() {
    if std::env::var("TRILITHON_E2E_CADDY").as_deref() != Ok("1") {
        return;
    }

    let events: Arc<Mutex<Vec<(String, Instant)>>> = Arc::default();
    let layer = EventCaptureLayer {
        events: Arc::clone(&events),
    };
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    // Set up an in-memory store.
    let opts = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite://:memory:")
        .unwrap()
        .create_if_missing(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(opts)
        .await
        .unwrap();
    apply_migrations(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO caddy_instances \
         (id, display_name, transport, address, created_at, ownership_token) \
         VALUES ('e2e-inst', 'E2E', 'unix', '/tmp/e2e.sock', 0, 'tok')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let store = CapabilityStore::new(pool);
    let cache = Arc::new(CapabilityCache::default());
    let test_start = Instant::now();

    let client = Arc::new(ScriptedClient {
        dead_window: (Duration::from_secs(2), Duration::from_secs(7)),
        start: test_start,
        modules: LoadedModules {
            modules: BTreeSet::from(["http.handlers.reverse_proxy".to_owned()]),
            caddy_version: "v2.8.4".to_owned(),
        },
    });

    // Run for at most 45 s — enough to observe reconnect + probe.
    let shutdown = TimedShutdown {
        deadline: test_start + Duration::from_secs(45),
    };

    reconnect_loop(
        client,
        Arc::clone(&cache),
        store,
        "e2e-inst".to_owned(),
        shutdown,
    )
    .await;

    // Find the restart time (first `caddy.connected` event).
    let captured = events.lock().unwrap().clone();
    let reconnect_ts = captured
        .iter()
        .find(|(msg, _)| msg == "caddy.connected")
        .map(|(_, ts)| *ts)
        .expect("caddy.connected event not emitted");

    // Verify a probe event followed the reconnect within 35 s.
    let probe_ts = captured
        .iter()
        .find(|(msg, ts)| msg == "caddy.capability-probe.completed" && *ts >= reconnect_ts)
        .map(|(_, ts)| *ts)
        .expect("caddy.capability-probe.completed not emitted after reconnect");

    let lag = probe_ts.duration_since(reconnect_ts);
    assert!(
        lag <= Duration::from_secs(35),
        "probe took {lag:?} after reconnect, want ≤ 35 s"
    );
}

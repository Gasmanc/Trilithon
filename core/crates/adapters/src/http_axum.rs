//! `axum`-based HTTP server — implementation of [`trilithon_core::http::HttpServer`].
//!
//! Binds `127.0.0.1:<port>` by default. Remote binding requires
//! `allow_remote_binding = true` and emits a stark warning at startup
//! (ADR-0011, architecture §8.1).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use serde::Serialize;
use serde_json::{Map, Value};
use tokio::net::TcpListener;
use trilithon_core::config::types::ServerConfig;
use trilithon_core::http::{HttpServer, HttpServerError, ShutdownSignal};

// ── AppState ──────────────────────────────────────────────────────────────────

/// Shared state threaded through every axum handler.
#[derive(Clone)]
pub struct AppState {
    /// Set to `true` while a Caddy config-write (apply) is in flight.
    pub apply_in_flight: Arc<AtomicBool>,
    /// Unix timestamp (ms) at which the daemon became ready; 0 = starting.
    pub ready_since_unix_ms: Arc<AtomicU64>,
}

// ── Health handler ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct HealthReady {
    status: &'static str,
    trilithon_version: &'static str,
    ready_since_unix_ms: u128,
    apply_in_flight: bool,
}

#[derive(Serialize)]
struct HealthStarting {
    status: &'static str,
}

/// Handler for `GET /api/v1/health`.
///
/// Returns 200 once the daemon is ready, 503 while still starting.
async fn health_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HealthReady>, (StatusCode, Json<HealthStarting>)> {
    let ready_since = state.ready_since_unix_ms.load(Ordering::Acquire);

    if ready_since == 0 {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthStarting { status: "starting" }),
        ));
    }

    Ok(Json(HealthReady {
        status: "ready",
        trilithon_version: env!("CARGO_PKG_VERSION"),
        ready_since_unix_ms: u128::from(ready_since),
        apply_in_flight: state.apply_in_flight.load(Ordering::Acquire),
    }))
}

/// Mark the daemon as ready by recording the current Unix timestamp in
/// milliseconds into the shared atomic.
pub fn mark_ready(ready_ms: &Arc<AtomicU64>) {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    // Clamp to u64 max (year ~584_938_606); safe in practice.
    #[allow(clippy::cast_possible_truncation)]
    ready_ms.store(ms as u64, Ordering::Release);
}

// ── OpenAPI placeholder ───────────────────────────────────────────────────────

/// Handler for `GET /api/v1/openapi.json`.
///
/// Returns a minimal, valid `OpenAPI` 3.1.0 document. Slice 9.11 fills in the
/// paths and schemas from `utoipa` generated output.
async fn openapi_placeholder() -> Json<Value> {
    let mut info = Map::new();
    info.insert(
        "title".to_owned(),
        Value::String("Trilithon Daemon API".to_owned()),
    );
    info.insert(
        "version".to_owned(),
        Value::String(env!("CARGO_PKG_VERSION").to_owned()),
    );

    let mut doc = Map::new();
    doc.insert("openapi".to_owned(), Value::String("3.1.0".to_owned()));
    doc.insert("info".to_owned(), Value::Object(info));
    doc.insert("paths".to_owned(), Value::Object(Map::new()));

    Json(Value::Object(doc))
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Build the axum router with all registered routes.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/openapi.json", get(openapi_placeholder))
        .with_state(state)
}

// ── Server config ─────────────────────────────────────────────────────────────

/// Configuration for [`AxumServer`].
#[derive(Clone, Debug)]
pub struct AxumServerConfig {
    /// Bind host. Default `"127.0.0.1"`.
    pub bind_host: String,
    /// Bind port. Default `7878`.
    pub bind_port: u16,
    /// If `false` (default), binding to a non-loopback address is rejected.
    pub allow_remote_binding: bool,
    /// Session cookie name. Default `"trilithon_session"`.
    pub session_cookie_name: String,
    /// Session TTL in seconds. Default `12 * 3600`.
    pub session_ttl_seconds: u64,
}

impl Default for AxumServerConfig {
    fn default() -> Self {
        Self {
            bind_host: "127.0.0.1".to_owned(),
            bind_port: 7878,
            allow_remote_binding: false,
            session_cookie_name: "trilithon_session".to_owned(),
            session_ttl_seconds: 12 * 3600,
        }
    }
}

// ── AxumServer ────────────────────────────────────────────────────────────────

/// The running HTTP server. Built from [`AxumServerConfig`].
pub struct AxumServer {
    config: AxumServerConfig,
    state: Arc<AppState>,
    listener: Option<TcpListener>,
    bound_addr: Option<SocketAddr>,
}

impl AxumServer {
    /// Create a new server with the given config and shared state.
    pub const fn new(config: AxumServerConfig, state: Arc<AppState>) -> Self {
        Self {
            config,
            state,
            listener: None,
            bound_addr: None,
        }
    }
}

/// Returns `true` if the host string parses as a loopback IP address.
fn is_loopback(addr: &str) -> bool {
    addr.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

#[async_trait]
impl HttpServer for AxumServer {
    async fn bind(&mut self, _config: &ServerConfig) -> Result<SocketAddr, HttpServerError> {
        let host = self.config.bind_host.clone();
        let port = self.config.bind_port;

        // H1 mitigation: reject non-loopback unless explicitly allowed.
        if !is_loopback(&host) && !self.config.allow_remote_binding {
            return Err(HttpServerError::BindFailed {
                detail: "remote binding requires network.allow_remote_binding = true".to_owned(),
            });
        }

        if !is_loopback(&host) && self.config.allow_remote_binding {
            tracing::warn!(
                "binding to non-loopback interface; authentication is required for every endpoint"
            );
        }

        let addr_str = format!("{host}:{port}");
        let listener =
            TcpListener::bind(&addr_str)
                .await
                .map_err(|e| HttpServerError::BindFailed {
                    detail: e.to_string(),
                })?;

        let addr = listener
            .local_addr()
            .map_err(|e| HttpServerError::BindFailed {
                detail: e.to_string(),
            })?;

        tracing::info!(target: "daemon.started", bind = %addr, "http server bound");

        self.listener = Some(listener);
        self.bound_addr = Some(addr);
        Ok(addr)
    }

    async fn run(mut self, shutdown: ShutdownSignal) -> Result<(), HttpServerError> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| HttpServerError::Crashed {
                detail: "run() called before bind()".to_owned(),
            })?;

        let app = router(self.state);

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| HttpServerError::Crashed {
                detail: e.to_string(),
            })
    }

    async fn shutdown(&self) -> Result<(), HttpServerError> {
        // Graceful shutdown is handled by the ShutdownSignal future passed to
        // `run`. This method exists for callers that need to signal externally;
        // the signal channel used by the CLI already covers this path.
        Ok(())
    }
}

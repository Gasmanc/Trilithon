//! Start the server, mark it ready, then assert `GET /api/v1/health` returns 200.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: integration test — panics and expect are the correct failure mode here

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Duration;

use trilithon_adapters::http_axum::{AppState, AxumServer, AxumServerConfig, mark_ready};
use trilithon_core::config::types::ServerConfig;
use trilithon_core::http::HttpServer;

fn make_state() -> Arc<AppState> {
    Arc::new(AppState {
        apply_in_flight: Arc::new(AtomicBool::new(false)),
        ready_since_unix_ms: Arc::new(AtomicU64::new(0)),
    })
}

#[tokio::test]
async fn http_health_returns_200() {
    let state = make_state();

    // Mark ready before the server starts so the health check passes.
    mark_ready(&state.ready_since_unix_ms);

    let cfg = AxumServerConfig {
        bind_port: 0, // OS assigns an available port
        ..AxumServerConfig::default()
    };
    let mut server = AxumServer::new(cfg, state);
    let server_cfg = ServerConfig {
        bind: "127.0.0.1:0".parse().expect("valid addr"),
        allow_remote: false,
    };
    let addr = server.bind(&server_cfg).await.expect("bind");

    // Channel to drive graceful shutdown.
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown = Box::pin(async move {
        let _ = rx.await;
    });

    tokio::spawn(async move {
        server.run(shutdown).await.ok();
    });

    // Wait briefly for the server to start accepting.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let url = format!("http://{addr}/api/v1/health");
    let resp = tokio::time::timeout(Duration::from_secs(5), reqwest::get(&url))
        .await
        .expect("request did not time out")
        .expect("HTTP request succeeded");

    assert_eq!(resp.status(), 200, "health must return 200");

    let body: serde_json::Value = resp.json().await.expect("valid JSON");
    assert_eq!(body["status"], "ready");

    // Shut down the server.
    let _ = tx.send(());
}

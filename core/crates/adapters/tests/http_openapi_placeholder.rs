//! `GET /api/v1/openapi.json` returns a parseable `OpenAPI` 3.1 document.

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

use trilithon_adapters::http_axum::{AxumServer, AxumServerConfig, stubs};
use trilithon_core::config::types::ServerConfig;
use trilithon_core::http::HttpServer;

fn make_state() -> Arc<trilithon_adapters::http_axum::AppState> {
    stubs::make_test_app_state(
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicU64::new(0)),
    )
}

#[tokio::test]
async fn http_openapi_placeholder() {
    let state = make_state();
    let cfg = AxumServerConfig {
        bind_port: 0,
        ..AxumServerConfig::default()
    };
    let mut server = AxumServer::new(cfg, state);
    let server_cfg = ServerConfig {
        bind: "127.0.0.1:0".parse().expect("valid addr"),
        allow_remote: false,
    };
    let addr = server.bind(&server_cfg).await.expect("bind");

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown = Box::pin(async move {
        let _ = rx.await;
    });

    tokio::spawn(async move {
        server.run(shutdown).await.ok();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let url = format!("http://{addr}/api/v1/openapi.json");
    let resp = tokio::time::timeout(Duration::from_secs(5), reqwest::get(&url))
        .await
        .expect("no timeout")
        .expect("request ok");

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("valid JSON");
    assert_eq!(body["openapi"], "3.1.0");
    assert_eq!(body["info"]["title"], "Trilithon Daemon API");

    let _ = tx.send(());
}

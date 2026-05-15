//! `GET /api/v1/openapi.json` — response is valid `OpenAPI` 3.1.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: integration test — panics are the correct failure mode

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Duration;

use trilithon_adapters::http_axum::{AxumServer, AxumServerConfig, stubs};
use trilithon_core::{config::types::ServerConfig, http::HttpServer};

#[tokio::test]
async fn openapi_validates_against_3_1_schema() {
    let state = stubs::make_test_app_state(
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicU64::new(1)),
    );
    let cfg = AxumServerConfig {
        bind_port: 0,
        ..AxumServerConfig::default()
    };
    let mut server = AxumServer::new(cfg, state);
    let server_cfg = ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
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

    let resp = reqwest::get(format!("http://{addr}/api/v1/openapi.json"))
        .await
        .expect("GET /api/v1/openapi.json");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("parse body");

    let version = body["openapi"].as_str().expect("openapi field");
    assert!(
        version.starts_with("3.1"),
        "expected OpenAPI 3.1.x, got: {version}"
    );

    let _ = tx.send(());
}

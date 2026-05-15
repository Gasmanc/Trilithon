//! `GET /api/v1/openapi.json` — all expected paths are present.

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

const EXPECTED_PATHS: &[&str] = &[
    "/api/v1/health",
    "/api/v1/auth/login",
    "/api/v1/auth/logout",
    "/api/v1/auth/change-password",
    "/api/v1/mutations",
    "/api/v1/snapshots",
    "/api/v1/snapshots/{id}",
    "/api/v1/snapshots/{a}/diff/{b}",
    "/api/v1/routes",
    "/api/v1/audit",
    "/api/v1/drift/current",
    "/api/v1/drift/{event_id}/adopt",
    "/api/v1/drift/{event_id}/reapply",
    "/api/v1/drift/{event_id}/defer",
    "/api/v1/capabilities",
];

#[tokio::test]
async fn openapi_documents_every_handler() {
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
    let paths = body["paths"].as_object().expect("paths object");

    for expected in EXPECTED_PATHS {
        assert!(
            paths.contains_key(*expected),
            "missing path in OpenAPI document: {expected}\nfound paths: {:?}",
            paths.keys().collect::<Vec<_>>()
        );
    }

    let _ = tx.send(());
}

//! Setting `bind_host = "0.0.0.0"` and `allow_remote_binding = false` must
//! cause `bind` to return `BindFailed`.

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

use trilithon_adapters::http_axum::{AxumServer, AxumServerConfig, stubs};
use trilithon_core::config::types::ServerConfig;
use trilithon_core::http::{HttpServer, HttpServerError};

fn dummy_state() -> Arc<trilithon_adapters::http_axum::AppState> {
    stubs::make_test_app_state(
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicU64::new(0)),
    )
}

#[tokio::test]
async fn http_remote_bind_rejected_without_flag() {
    let cfg = AxumServerConfig {
        bind_host: "0.0.0.0".to_owned(),
        allow_remote_binding: false,
        ..AxumServerConfig::default()
    };
    let mut server = AxumServer::new(cfg, dummy_state());
    let server_cfg = ServerConfig {
        bind: "127.0.0.1:7878".parse().expect("valid addr"),
        allow_remote: false,
    };
    let result = server.bind(&server_cfg).await;
    assert!(
        matches!(result, Err(HttpServerError::BindFailed { .. })),
        "expected BindFailed, got {result:?}"
    );
}

//! When `allow_remote_binding = true` and host is non-loopback, a warn-level
//! tracing event must be emitted.
//!
//! We use `0.0.0.0:0` as the non-loopback bind target so the OS can pick any
//! available port. If the bind fails (restricted env), the test skips the
//! warn assertion but still verifies the `BindFailed` path is not taken.

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
use trilithon_core::http::HttpServer;

fn dummy_state() -> Arc<trilithon_adapters::http_axum::AppState> {
    stubs::make_test_app_state(
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicU64::new(0)),
    )
}

/// Returns true if the global `tracing-test` buffer contains `needle`.
///
/// Unlike the macro-injected `logs_contain`, this bypasses scope filtering,
/// which is needed when the tracing event originates from an awaited future
/// (the `span.enter()` guard is suspended across `.await` points).
fn raw_logs_contain(needle: &str) -> bool {
    let logs = {
        let buf = tracing_test::internal::global_buf().lock().expect("lock");
        String::from_utf8(buf.clone()).expect("utf8")
    };
    logs.contains(needle)
}

#[tracing_test::traced_test]
#[tokio::test]
async fn http_remote_bind_warns_with_flag() {
    let cfg = AxumServerConfig {
        bind_host: "0.0.0.0".to_owned(),
        allow_remote_binding: true,
        bind_port: 0,
        ..AxumServerConfig::default()
    };
    let mut server = AxumServer::new(cfg, dummy_state());
    let server_cfg = ServerConfig {
        bind: "0.0.0.0:0".parse().expect("valid addr"),
        allow_remote: true,
    };

    match server.bind(&server_cfg).await {
        Ok(addr) => {
            // Bind succeeded — the warn must have been emitted.
            assert!(addr.port() > 0, "bound to a real port");
            assert!(
                raw_logs_contain("binding to non-loopback interface"),
                "warn must be emitted when allow_remote_binding = true"
            );
        }
        Err(e) => {
            // Some CI environments block binding to 0.0.0.0. Accept this case
            // but verify the error is not the "flag required" rejection (that
            // would mean the flag was ignored).
            let msg = e.to_string();
            assert!(
                !msg.contains("allow_remote_binding"),
                "error must not be the 'flag required' message; got: {msg}"
            );
        }
    }
}

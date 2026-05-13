//! `AxumServerConfig::default().bind_host == "127.0.0.1"`.

use trilithon_adapters::http_axum::AxumServerConfig;

#[test]
fn http_loopback_default() {
    let cfg = AxumServerConfig::default();
    assert_eq!(cfg.bind_host, "127.0.0.1");
    assert_eq!(cfg.bind_port, 7878);
    assert!(!cfg.allow_remote_binding);
}

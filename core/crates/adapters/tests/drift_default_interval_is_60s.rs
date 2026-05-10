//! Slice 8.5 — default interval is 60 seconds.

use std::time::Duration;

use trilithon_adapters::drift::DriftDetectorConfig;

#[test]
fn drift_default_interval_is_60s() {
    let config = DriftDetectorConfig::default();
    assert_eq!(config.interval, Duration::from_secs(60));
}

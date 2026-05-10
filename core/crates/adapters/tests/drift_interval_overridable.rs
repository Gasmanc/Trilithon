//! Slice 8.5 — interval is configurable and validated.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
// reason: integration test

use std::time::Duration;

use trilithon_adapters::drift::DriftDetectorConfig;

#[test]
fn drift_interval_overridable() {
    let config = DriftDetectorConfig {
        interval: Duration::from_secs(10),
        instance_id: "local".into(),
    };
    assert_eq!(config.interval, Duration::from_secs(10));
    config.validate().expect("10s should be valid");

    // Verify default is different.
    let default = DriftDetectorConfig::default();
    assert_ne!(default.interval, config.interval);
}

#[test]
fn drift_interval_below_minimum_rejected() {
    let config = DriftDetectorConfig {
        interval: Duration::from_secs(5),
        instance_id: "local".into(),
    };
    assert!(
        config.validate().is_err(),
        "5s should be below minimum (10s)"
    );
}

#[test]
fn drift_interval_above_maximum_rejected() {
    let config = DriftDetectorConfig {
        interval: Duration::from_secs(7200),
        instance_id: "local".into(),
    };
    assert!(
        config.validate().is_err(),
        "7200s should be above maximum (3600s)"
    );
}

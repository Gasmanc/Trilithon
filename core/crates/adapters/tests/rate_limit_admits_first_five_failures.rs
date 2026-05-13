//! Five failures from the same address are all admitted; the sixth is rejected.

#![allow(clippy::unwrap_used, clippy::disallowed_methods)]
// reason: test-only

use std::net::IpAddr;

use trilithon_adapters::auth::LoginRateLimiter;

#[test]
fn rate_limit_admits_first_five_failures() {
    let limiter = LoginRateLimiter::new();
    let addr: IpAddr = "10.0.0.1".parse().unwrap();
    let now = 1_000_000_i64;

    // First five failures: check must pass, then record failure.
    for i in 0..5 {
        assert!(
            limiter.check(addr, now).is_ok(),
            "failure {} should be admitted",
            i + 1
        );
        limiter.record_failure(addr, now);
    }

    // Sixth check must be rejected.
    assert!(
        limiter.check(addr, now).is_err(),
        "sixth attempt must be rejected"
    );
}

#[test]
fn rate_limit_success_resets_bucket() {
    let limiter = LoginRateLimiter::new();
    let addr: IpAddr = "10.0.0.2".parse().unwrap();
    let now = 1_000_000_i64;

    for _ in 0..6 {
        limiter.record_failure(addr, now);
    }
    assert!(limiter.check(addr, now).is_err());

    limiter.record_success(addr);
    assert!(
        limiter.check(addr, now).is_ok(),
        "success must reset the bucket"
    );
}

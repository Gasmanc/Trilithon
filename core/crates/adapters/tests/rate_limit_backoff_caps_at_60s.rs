//! Driving the bucket many failures past the threshold must not produce a
//! retry-after greater than 60 seconds.

#![allow(clippy::unwrap_used, clippy::disallowed_methods, clippy::panic)]
// reason: test-only

use std::net::IpAddr;

use trilithon_adapters::auth::LoginRateLimiter;

#[test]
fn rate_limit_backoff_caps_at_60s() {
    let limiter = LoginRateLimiter::new();
    let addr: IpAddr = "10.0.0.3".parse().unwrap();
    let now = 0_i64;

    // Drive well past 2^11 = 2048 to make sure the cap kicks in.
    for _ in 0..20 {
        limiter.record_failure(addr, now);
    }

    match limiter.check(addr, now) {
        Err(err) => assert!(
            err.retry_after_seconds <= 60,
            "retry_after must not exceed 60 s, got {}",
            err.retry_after_seconds
        ),
        Ok(()) => panic!("check must be rejected after many failures"),
    }
}

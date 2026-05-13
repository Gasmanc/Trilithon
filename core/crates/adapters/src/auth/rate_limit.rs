//! In-memory login rate limiter keyed by source IP address.
//!
//! Allows at most five consecutive failures per address per minute, then
//! applies exponential back-off capped at 60 seconds.

use std::net::IpAddr;

use dashmap::DashMap;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Returned by [`LoginRateLimiter::check`] when the request is rejected.
#[derive(Clone, Debug)]
pub struct RateLimited {
    /// Seconds the caller must wait before retrying.
    pub retry_after_seconds: u32,
}

// ---------------------------------------------------------------------------
// Per-address bucket
// ---------------------------------------------------------------------------

#[derive(Default)]
struct BucketState {
    failure_count: u32,
    next_allowed_at_unix: i64,
}

// ---------------------------------------------------------------------------
// Rate limiter
// ---------------------------------------------------------------------------

/// Tracks per-address failure counts and enforces back-off.
pub struct LoginRateLimiter {
    buckets: DashMap<IpAddr, BucketState>,
}

impl LoginRateLimiter {
    /// Create a new, empty rate limiter.
    pub fn new() -> Self {
        Self {
            buckets: DashMap::new(),
        }
    }

    /// Returns `Ok(())` when the request should be admitted, or `Err` with the
    /// back-off duration when the address is temporarily blocked.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimited`] when the address is within a back-off window.
    pub fn check(&self, addr: IpAddr, now_unix: i64) -> Result<(), RateLimited> {
        if let Some(bucket) = self.buckets.get(&addr) {
            if now_unix < bucket.next_allowed_at_unix {
                let secs = bucket.next_allowed_at_unix - now_unix;
                let retry_after = u32::try_from(secs.max(1)).unwrap_or(u32::MAX);
                return Err(RateLimited {
                    retry_after_seconds: retry_after,
                });
            }
        }
        Ok(())
    }

    /// Record a failed login attempt for `addr`.
    pub fn record_failure(&self, addr: IpAddr, now_unix: i64) {
        let mut bucket = self.buckets.entry(addr).or_default();
        bucket.failure_count += 1;
        if bucket.failure_count > 5 {
            let exponent = bucket.failure_count - 5;
            let backoff = 2_i64.saturating_pow(exponent).min(60);
            bucket.next_allowed_at_unix = now_unix + backoff;
        }
    }

    /// Record a successful login for `addr`, clearing the failure bucket.
    pub fn record_success(&self, addr: IpAddr) {
        self.buckets.remove(&addr);
    }
}

impl Default for LoginRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

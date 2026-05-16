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
        // After the 5th consecutive failure the address enters back-off.
        // Setting next_allowed_at_unix here means check() will reject the *next*
        // (6th and beyond) attempt before any handler logic runs.
        if bucket.failure_count >= 5 {
            let exponent = bucket.failure_count - 4;
            let backoff = 2_i64.saturating_pow(exponent).min(60);
            bucket.next_allowed_at_unix = now_unix + backoff;
        }
        // Lazy eviction: remove stale buckets whose back-off window ended more
        // than 5 minutes ago to bound map growth (F010). Run on 1% of writes to
        // amortise the scan cost without a background task.
        if bucket.failure_count % 100 == 0 {
            let cutoff = now_unix - 300;
            self.buckets.retain(|_, b| b.next_allowed_at_unix > cutoff);
        }
    }

    /// Record a successful login for `addr`, clearing the failure bucket.
    pub fn record_success(&self, addr: IpAddr) {
        self.buckets.remove(&addr);
    }

    /// Evict all stale buckets whose back-off window ended before `before_unix`.
    ///
    /// Safe to call from a periodic background task as an alternative to the
    /// lazy eviction in [`record_failure`].
    pub fn evict_stale(&self, before_unix: i64) {
        self.buckets
            .retain(|_, b| b.next_allowed_at_unix > before_unix);
    }
}

impl Default for LoginRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

//! Wall-clock abstraction for testable time-dependent logic.
//!
//! The single method [`Clock::now_unix_ms`] returns the current time as
//! milliseconds since the Unix epoch (UTC). Callers derive whole-second
//! timestamps by dividing by 1 000.

/// Provides the current time as milliseconds since the Unix epoch.
///
/// Implementations MUST be `Send + Sync + 'static` so they can be stored
/// behind `Arc<dyn Clock>`.
pub trait Clock: Send + Sync + 'static {
    /// Return the current time in milliseconds since the Unix epoch (UTC).
    fn now_unix_ms(&self) -> i64;
}

/// A [`Clock`] implementation that delegates to the system wall clock via
/// [`std::time::SystemTime`].
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_ms(&self) -> i64 {
        use std::time::SystemTime;
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| {
                // Saturating conversion: i64::MAX is year 292_277_026, well beyond
                // any realistic system time. Truncation to i64 is intentional.
                #[allow(clippy::cast_possible_truncation)]
                // reason: Unix epoch ms fits in i64 until year 292M; truncation is intentional
                {
                    i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
                }
            })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_plausible_value() {
        let clock = SystemClock;
        let ms = clock.now_unix_ms();
        // After 2020-01-01 00:00:00 UTC = 1_577_836_800_000 ms
        assert!(ms > 1_577_836_800_000, "expected a plausible Unix ms: {ms}");
    }
}

//! In-memory cache for the most-recently probed [`CaddyCapabilities`].

use parking_lot::RwLock;
use trilithon_core::caddy::capabilities::CaddyCapabilities;

/// Thread-safe, lock-free-read cache for the latest [`CaddyCapabilities`].
///
/// A single shared instance is held in an [`std::sync::Arc`] and passed to
/// both the probe runner (writer) and any code that reads capabilities at
/// request time (readers).
#[derive(Default)]
pub struct CapabilityCache {
    inner: RwLock<Option<CaddyCapabilities>>,
}

impl CapabilityCache {
    /// Return a clone of the currently cached capabilities, or `None` if the
    /// probe has not completed yet.
    pub fn snapshot(&self) -> Option<CaddyCapabilities> {
        self.inner.read().clone()
    }

    /// Replace the cached value with `value`.
    pub fn replace(&self, value: CaddyCapabilities) {
        *self.inner.write() = Some(value);
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn make_caps() -> CaddyCapabilities {
        CaddyCapabilities {
            loaded_modules: BTreeSet::from(["http.handlers.reverse_proxy".to_owned()]),
            caddy_version: "v2.8.4".to_owned(),
            probed_at: 1_700_000_000,
        }
    }

    #[test]
    fn empty_on_default() {
        let cache = CapabilityCache::default();
        assert!(cache.snapshot().is_none());
    }

    #[test]
    fn replace_then_snapshot() {
        let cache = CapabilityCache::default();
        let caps = make_caps();
        cache.replace(caps.clone());
        assert_eq!(cache.snapshot(), Some(caps));
    }

    #[test]
    fn replace_overwrites_previous() {
        let cache = CapabilityCache::default();
        cache.replace(make_caps());

        let newer = CaddyCapabilities {
            caddy_version: "v2.9.0".to_owned(),
            ..make_caps()
        };
        cache.replace(newer.clone());
        assert_eq!(cache.snapshot(), Some(newer));
    }
}

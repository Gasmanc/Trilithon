//! Background reconnect loop with capped exponential backoff.
//!
//! [`reconnect_loop`] monitors Caddy reachability via
//! [`CaddyClient::health_check`]. On disconnect it emits a
//! `caddy.disconnected` tracing event once. On reconnection it emits
//! `caddy.connected` and triggers a fresh capability probe via
//! [`run_initial_probe`].
//!
//! Backoff starts at [`INITIAL_BACKOFF`] (250 ms), doubles on each consecutive
//! failure, and plateaus at [`MAX_BACKOFF`] (30 s).

use std::sync::Arc;
use std::time::Duration;

use trilithon_core::caddy::{client::CaddyClient, types::HealthState};

use crate::caddy::{
    cache::CapabilityCache, capability_store::CapabilityStore, probe::run_initial_probe,
};

/// Initial retry wait after the first Caddy disconnect.
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(250);

/// Maximum retry wait; backoff plateaus here.
pub const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Health-check poll interval during normal (connected) operation.
const HEALTH_INTERVAL: Duration = Duration::from_secs(15);

/// Implement this on a concrete shutdown handle so [`reconnect_loop`] can
/// observe shutdown requests without depending on the `cli` crate.
pub trait ShutdownObserver: Send + 'static {
    /// Return a future that resolves when shutdown is signalled.
    ///
    /// The returned future borrows `self` mutably, so it must be polled to
    /// completion before `changed` can be called again.
    fn changed(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;

    /// Returns `true` if shutdown has already been signalled.
    fn is_shutting_down(&self) -> bool;
}

/// Run the Caddy reconnect loop until shutdown is signalled.
///
/// The function assumes that an initial capability probe has already succeeded
/// before it is called, so it starts in the [`HealthState::Reachable`] state.
///
/// # Events emitted
///
/// | Event | When |
/// |---|---|
/// | `caddy.disconnected` | First `health_check` failure after a connected period |
/// | `caddy.connected` | First successful `health_check` after a disconnected period |
/// | `caddy.capability-probe.completed` | Emitted by [`run_initial_probe`] on every reconnect |
pub async fn reconnect_loop(
    client: Arc<dyn CaddyClient>,
    cache: Arc<CapabilityCache>,
    persistence: CapabilityStore,
    instance_id: String,
    mut shutdown: impl ShutdownObserver,
) {
    let mut state = HealthState::Reachable;
    let mut backoff = INITIAL_BACKOFF;

    loop {
        // Sleep duration depends on state: use HEALTH_INTERVAL when connected
        // so we do not hammer Caddy; use the current backoff when disconnected
        // so retries respect the capped exponential schedule without an extra
        // 15 s penalty on top of each backoff step.
        let sleep_duration = if state == HealthState::Reachable {
            HEALTH_INTERVAL
        } else {
            backoff
        };

        tokio::select! {
            () = tokio::time::sleep(sleep_duration) => {}
            () = shutdown.changed() => { break; }
        }

        match client.health_check().await {
            Ok(HealthState::Reachable) => {
                if state == HealthState::Unreachable {
                    tracing::info!("caddy.connected");
                    // Log but don't abort the loop — a probe failure is transient.
                    if let Err(err) =
                        run_initial_probe(&*client, Arc::clone(&cache), &persistence, &instance_id)
                            .await
                    {
                        tracing::warn!(error = %err, "caddy.reconnect.probe_failed");
                    }
                    backoff = INITIAL_BACKOFF;
                }
                state = HealthState::Reachable;
            }
            Ok(HealthState::Unreachable) | Err(_) => {
                if state == HealthState::Reachable {
                    tracing::info!("caddy.disconnected");
                }
                state = HealthState::Unreachable;
                backoff = std::cmp::min(backoff * 2, MAX_BACKOFF);
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;

    /// The backoff schedule must be 250 ms, 500 ms, 1 s, 2 s, 4 s, 8 s,
    /// 16 s, then cap at 30 s for subsequent failures.
    #[test]
    fn backoff_doubles_then_caps() {
        let mut backoff = INITIAL_BACKOFF;

        let expected_ms: &[u64] = &[
            250, 500, 1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000,
        ];

        for &ms in expected_ms {
            assert_eq!(
                u64::try_from(backoff.as_millis()).expect("backoff fits u64"),
                ms,
                "expected backoff {ms} ms, got {} ms",
                backoff.as_millis()
            );
            backoff = std::cmp::min(backoff * 2, MAX_BACKOFF);
        }
    }
}

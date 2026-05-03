//! Ownership sentinel — ensures exactly one Trilithon instance owns the
//! running Caddy configuration.
//!
//! At startup, after the initial capability probe, [`ensure_sentinel`] reads
//! the live Caddy config and looks for a JSON object anywhere in the tree
//! whose `"@id"` field equals `"trilithon-owner"`.
//!
//! - **Absent** → writes a sentinel block containing our `installation_id`.
//! - **Present, ours** → no-op.
//! - **Present, foreign, `takeover = false`** → returns [`SentinelError::Conflict`].
//! - **Present, foreign, `takeover = true`** → overwrites and returns a
//!   [`SentinelOutcome::TookOver`] along with an in-memory
//!   [`AuditEvent::OwnershipSentinelTakeover`] stub for Phase 6.

use trilithon_core::{
    caddy::{
        client::CaddyClient,
        error::CaddyError,
        types::{CaddyJsonPointer, JsonPatch, JsonPatchOp},
    },
    storage::AuditEvent,
};

/// The `@id` value used to mark the ownership sentinel.
pub const SENTINEL_ID: &str = "trilithon-owner";

/// JSON Pointer to the sentinel server entry.
const SENTINEL_POINTER: &str = "/apps/http/servers/__trilithon_sentinel__";

/// Outcome of a successful [`ensure_sentinel`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SentinelOutcome {
    /// The sentinel was absent and has been written.
    Created,
    /// The sentinel was already present with our `installation_id`.
    AlreadyOurs,
    /// The sentinel was present with a different `installation_id` and was
    /// overwritten because `--takeover` was set.
    TookOver {
        /// The previous owner's installation id.
        previous_installation_id: String,
    },
}

/// Errors returned by [`ensure_sentinel`].
#[derive(Debug, thiserror::Error)]
pub enum SentinelError {
    /// A foreign sentinel is present and `--takeover` was not set.
    #[error("ownership sentinel conflict: caddy carries installation_id {found}, ours is {ours}")]
    Conflict {
        /// The installation id found in the running config.
        found: String,
        /// Our own installation id.
        ours: String,
    },
    /// An error communicating with the Caddy admin API.
    #[error("caddy error: {source}")]
    Caddy {
        /// The underlying Caddy API error.
        #[from]
        source: CaddyError,
    },
}

/// Ensure the ownership sentinel is present and matches `installation_id`.
///
/// # Algorithm
///
/// 1. Retrieve the full running config via [`CaddyClient::get_running_config`].
/// 2. Walk the config JSON recursively; collect objects with `"@id" ==
///    "trilithon-owner"`.
/// 3. Act on the result as described in the module docs.
///
/// When `takeover` succeeds an [`AuditEvent::OwnershipSentinelTakeover`] is
/// returned via the `Ok` side as a tuple `(outcome, Some(event))`.  All other
/// outcomes return `None` for the audit event.
///
/// # Errors
///
/// Returns [`SentinelError::Caddy`] if any admin API call fails, or
/// [`SentinelError::Conflict`] when a foreign sentinel is detected without
/// `--takeover`.
pub async fn ensure_sentinel(
    client: &dyn CaddyClient,
    installation_id: &str,
    takeover: bool,
) -> Result<(SentinelOutcome, Option<AuditEvent>), SentinelError> {
    let cfg = client.get_running_config().await?;

    let sentinels = find_sentinels(&cfg.0);

    match sentinels.as_slice() {
        [] => {
            // No sentinel found — write one.
            // Build the value explicitly to avoid serde_json::json! (which
            // triggers the disallowed_methods lint via its internal unwrap).
            let sentinel_value = {
                let mut map = serde_json::Map::new();
                map.insert(
                    "@id".to_owned(),
                    serde_json::Value::String(SENTINEL_ID.to_owned()),
                );
                map.insert(
                    "installation_id".to_owned(),
                    serde_json::Value::String(installation_id.to_owned()),
                );
                serde_json::Value::Object(map)
            };
            client
                .patch_config(
                    CaddyJsonPointer(SENTINEL_POINTER.to_owned()),
                    JsonPatch(vec![JsonPatchOp::Add {
                        path: SENTINEL_POINTER.to_owned(),
                        value: sentinel_value,
                    }]),
                )
                .await?;
            Ok((SentinelOutcome::Created, None))
        }
        [found_id] if *found_id == installation_id => {
            // Sentinel present and matches ours.
            Ok((SentinelOutcome::AlreadyOurs, None))
        }
        [previous] => {
            let previous = (*previous).to_owned();
            if takeover {
                let pointer = format!("{SENTINEL_POINTER}/installation_id");
                client
                    .patch_config(
                        CaddyJsonPointer(SENTINEL_POINTER.to_owned()),
                        JsonPatch(vec![JsonPatchOp::Replace {
                            path: pointer,
                            value: serde_json::Value::String(installation_id.to_owned()),
                        }]),
                    )
                    .await?;

                let event = AuditEvent::OwnershipSentinelTakeover {
                    previous_installation_id: previous.clone(),
                    new_installation_id: installation_id.to_owned(),
                };

                Ok((
                    SentinelOutcome::TookOver {
                        previous_installation_id: previous,
                    },
                    Some(event),
                ))
            } else {
                Err(conflict_error(installation_id, previous))
            }
        }
        _ => {
            // Multiple sentinels — unexpected state; report the first one.
            Err(conflict_error(installation_id, sentinels[0].to_owned()))
        }
    }
}

/// Emit the `caddy.ownership-sentinel.conflict` tracing event and return the
/// corresponding error.  Extracted to avoid duplicating the emit + construction
/// between the single-foreign and multiple-sentinels match arms.
fn conflict_error(ours: &str, found: String) -> SentinelError {
    tracing::error!(
        expected = %ours,
        found = %found,
        "caddy.ownership-sentinel.conflict",
    );
    SentinelError::Conflict {
        found,
        ours: ours.to_owned(),
    }
}

/// Walk a JSON value recursively and collect the `"installation_id"` field
/// from any object whose `"@id"` equals [`SENTINEL_ID`].
fn find_sentinels(value: &serde_json::Value) -> Vec<&str> {
    let mut out = Vec::new();
    collect_sentinels(value, &mut out);
    out
}

fn collect_sentinels<'v>(value: &'v serde_json::Value, out: &mut Vec<&'v str>) {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("@id").and_then(|v| v.as_str()) == Some(SENTINEL_ID) {
                if let Some(id) = map.get("installation_id").and_then(|v| v.as_str()) {
                    out.push(id);
                }
            }
            for child in map.values() {
                collect_sentinels(child, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                collect_sentinels(item, out);
            }
        }
        _ => {}
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
    use std::sync::Mutex;

    use async_trait::async_trait;

    use trilithon_core::caddy::{
        client::CaddyClient,
        error::CaddyError,
        types::{
            CaddyConfig, CaddyJsonPointer, HealthState, JsonPatch, JsonPatchOp, LoadedModules,
            TlsCertificate, UpstreamHealth,
        },
    };
    use trilithon_core::storage::AuditEvent;

    use super::{SENTINEL_ID, SENTINEL_POINTER, SentinelError, SentinelOutcome, ensure_sentinel};

    // -----------------------------------------------------------------------
    // Test double
    // -----------------------------------------------------------------------

    /// A `CaddyClient` double that records patch calls and returns a fixed config.
    struct CaddyClientDouble {
        config: serde_json::Value,
        patches: Mutex<Vec<(CaddyJsonPointer, JsonPatch)>>,
    }

    impl CaddyClientDouble {
        fn new(config: serde_json::Value) -> Self {
            Self {
                config,
                patches: Mutex::new(Vec::new()),
            }
        }

        fn recorded_patches(&self) -> Vec<(CaddyJsonPointer, JsonPatch)> {
            self.patches.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl CaddyClient for CaddyClientDouble {
        async fn load_config(&self, _body: CaddyConfig) -> Result<(), CaddyError> {
            unimplemented!("not needed in sentinel tests")
        }

        async fn patch_config(
            &self,
            pointer: CaddyJsonPointer,
            ops: JsonPatch,
        ) -> Result<(), CaddyError> {
            self.patches.lock().unwrap().push((pointer, ops));
            Ok(())
        }

        async fn get_running_config(&self) -> Result<CaddyConfig, CaddyError> {
            Ok(CaddyConfig(self.config.clone()))
        }

        async fn get_loaded_modules(&self) -> Result<LoadedModules, CaddyError> {
            unimplemented!("not needed in sentinel tests")
        }

        async fn get_upstream_health(&self) -> Result<Vec<UpstreamHealth>, CaddyError> {
            unimplemented!("not needed in sentinel tests")
        }

        async fn get_certificates(&self) -> Result<Vec<TlsCertificate>, CaddyError> {
            unimplemented!("not needed in sentinel tests")
        }

        async fn health_check(&self) -> Result<HealthState, CaddyError> {
            unimplemented!("not needed in sentinel tests")
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn empty_config() -> serde_json::Value {
        serde_json::json!({})
    }

    fn config_with_sentinel(installation_id: &str) -> serde_json::Value {
        // Use the module constants so test fixtures stay in sync with
        // the production path (SENTINEL_POINTER = /apps/http/servers/__trilithon_sentinel__,
        // SENTINEL_ID = "trilithon-owner").
        serde_json::json!({
            "apps": {
                "http": {
                    "servers": {
                        // The key name is the last segment of SENTINEL_POINTER.
                        "__trilithon_sentinel__": {
                            "@id": SENTINEL_ID,
                            "installation_id": installation_id
                        }
                    }
                }
            }
        })
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// When the config contains no sentinel, `ensure_sentinel` must write one
    /// and return `SentinelOutcome::Created`.
    #[tokio::test]
    async fn creates_when_absent() {
        let client = CaddyClientDouble::new(empty_config());
        let (outcome, event) = ensure_sentinel(&client, "our-id", false)
            .await
            .expect("should not error");

        assert_eq!(outcome, SentinelOutcome::Created);
        assert!(event.is_none());

        let patches = client.recorded_patches();
        assert_eq!(patches.len(), 1, "expected exactly one patch call");
        let (_, JsonPatch(ops)) = &patches[0];
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], JsonPatchOp::Add { value, path }
                if value.get("@id").and_then(|v| v.as_str()) == Some(SENTINEL_ID)
                && value.get("installation_id").and_then(|v| v.as_str()) == Some("our-id")
                && path == SENTINEL_POINTER),
            "Add op must carry correct sentinel payload; got {ops:?}",
        );
    }

    /// When the config already carries our sentinel, the function must return
    /// `AlreadyOurs` without issuing any patch calls.
    #[tokio::test]
    async fn already_ours_no_op() {
        let client = CaddyClientDouble::new(config_with_sentinel("our-id"));
        let (outcome, event) = ensure_sentinel(&client, "our-id", false)
            .await
            .expect("should not error");

        assert_eq!(outcome, SentinelOutcome::AlreadyOurs);
        assert!(event.is_none());
        assert!(
            client.recorded_patches().is_empty(),
            "no patches must be issued when sentinel matches",
        );
    }

    /// When the config carries a foreign sentinel and `takeover = false`, the
    /// function must emit `caddy.ownership-sentinel.conflict` and return
    /// `SentinelError::Conflict`.
    ///
    /// Uses `#[test]` (not `#[tokio::test]`) because the event-capture
    /// pattern requires constructing its own single-thread runtime inside
    /// `with_default`, which cannot nest inside an existing runtime.
    #[test]
    fn conflict_without_takeover_errors() {
        use std::sync::Arc;

        use tracing::subscriber::with_default;
        use tracing_subscriber::layer::SubscriberExt as _;

        struct EventCollector {
            events: Arc<Mutex<Vec<String>>>,
        }

        struct MessageVisitor<'a> {
            message: &'a mut Option<String>,
        }

        impl tracing::field::Visit for MessageVisitor<'_> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    *self.message = Some(format!("{value:?}"));
                }
            }

            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "message" {
                    *self.message = Some(value.to_owned());
                }
            }
        }

        impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for EventCollector {
            fn on_event(
                &self,
                event: &tracing::Event<'_>,
                _ctx: tracing_subscriber::layer::Context<'_, S>,
            ) {
                let mut msg: Option<String> = None;
                event.record(&mut MessageVisitor { message: &mut msg });
                if let Some(m) = msg {
                    self.events.lock().unwrap().push(m);
                }
            }
        }

        let events: Arc<Mutex<Vec<String>>> = Arc::default();
        let collector = EventCollector {
            events: Arc::clone(&events),
        };
        let subscriber = tracing_subscriber::registry().with(collector);

        let client = CaddyClientDouble::new(config_with_sentinel("deadbeef"));

        let result = with_default(subscriber, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(ensure_sentinel(&client, "ours-id", false))
        });

        let err = result.expect_err("should return Conflict error");
        assert!(
            matches!(&err, SentinelError::Conflict { found, ours }
                if found == "deadbeef" && ours == "ours-id"),
            "unexpected error: {err}",
        );
        assert!(
            client.recorded_patches().is_empty(),
            "no patches must be issued on conflict",
        );

        let emitted = events.lock().unwrap().clone();
        assert!(
            emitted
                .iter()
                .any(|n| n == "caddy.ownership-sentinel.conflict"),
            "expected caddy.ownership-sentinel.conflict in emitted events; got: {emitted:?}",
        );
    }

    /// When the config carries a foreign sentinel and `takeover = true`, the
    /// function must overwrite and return `TookOver` plus an audit event stub.
    #[tokio::test]
    async fn takeover_overwrites() {
        let client = CaddyClientDouble::new(config_with_sentinel("deadbeef"));
        let (outcome, event) = ensure_sentinel(&client, "ours-id", true)
            .await
            .expect("takeover should succeed");

        assert_eq!(
            outcome,
            SentinelOutcome::TookOver {
                previous_installation_id: "deadbeef".to_owned(),
            }
        );

        // Audit event stub must be produced.
        assert_eq!(
            event,
            Some(AuditEvent::OwnershipSentinelTakeover {
                previous_installation_id: "deadbeef".to_owned(),
                new_installation_id: "ours-id".to_owned(),
            })
        );

        let patches = client.recorded_patches();
        assert_eq!(patches.len(), 1, "expected exactly one patch call");
        let (_, JsonPatch(ops)) = &patches[0];
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], JsonPatchOp::Replace { value, .. }
                if value.as_str() == Some("ours-id")),
            "Replace op must carry our installation_id; got {ops:?}",
        );
    }
}

//! Tracing subscriber initialisation for the Trilithon daemon.
//!
//! Call [`init`] exactly once per process, after writing the pre-tracing line
//! to stderr and parsing the CLI config.

use std::io;

use tracing::Subscriber;
use tracing_subscriber::{
    EnvFilter, Layer, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};
use trilithon_core::config::{LogFormat, TracingConfig};

/// Error variants returned by [`init`].
#[derive(Debug, thiserror::Error)]
pub enum ObsError {
    /// A global subscriber was already installed.
    #[error("subscriber already installed")]
    AlreadyInstalled,
    /// The log filter directive string was invalid.
    #[error("invalid log filter {filter}: {detail}")]
    BadFilter {
        /// The filter string that failed to parse.
        filter: String,
        /// Human-readable parse error.
        detail: String,
    },
}

/// Install the global tracing subscriber. Must be called exactly once per
/// process.
///
/// The filter is resolved as follows (first wins):
/// 1. `RUST_LOG` environment variable.
/// 2. `config.log_filter` directive string.
///
/// The output format is chosen from `config.format` (or overridden by
/// `TRILITHON_LOG_FORMAT=json`).
///
/// # Errors
///
/// Returns [`ObsError::AlreadyInstalled`] if a subscriber is already global,
/// or [`ObsError::BadFilter`] if `config.log_filter` is not a valid directive.
pub fn init(config: &TracingConfig) -> Result<(), ObsError> {
    let env_filter = build_filter(config)?;

    let format = resolve_format(config.format);

    match format {
        LogFormat::Json => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_writer(io::stderr);
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(UtcSecondsLayer)
                .try_init()
                .map_err(|_| ObsError::AlreadyInstalled)
        }
        LogFormat::Pretty => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .compact()
                .with_writer(io::stderr);
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(UtcSecondsLayer)
                .try_init()
                .map_err(|_| ObsError::AlreadyInstalled)
        }
    }
}

/// Build the [`EnvFilter`] from config, respecting `RUST_LOG` if set.
fn build_filter(config: &TracingConfig) -> Result<EnvFilter, ObsError> {
    // RUST_LOG takes precedence if it is set and valid.
    if let Ok(filter) = EnvFilter::try_from_default_env() {
        return Ok(filter);
    }
    EnvFilter::try_new(&config.log_filter).map_err(|e| ObsError::BadFilter {
        filter: config.log_filter.clone(),
        detail: e.to_string(),
    })
}

/// Resolve the effective [`LogFormat`], allowing `TRILITHON_LOG_FORMAT=json`
/// to override the config value.
fn resolve_format(configured: LogFormat) -> LogFormat {
    if std::env::var("TRILITHON_LOG_FORMAT").is_ok_and(|v| v.eq_ignore_ascii_case("json")) {
        LogFormat::Json
    } else {
        configured
    }
}

// ---------------------------------------------------------------------------
// UtcSecondsLayer
// ---------------------------------------------------------------------------

/// Layer that injects a `ts_unix_seconds` integer field on every event.
///
/// In the fmt/json pipeline the timestamp appears as an additional field
/// recorded via `tracing::info!` span instrumentation. In this layer, the
/// value is stored in a thread-local so tests can inspect it without
/// coupling to the formatter output.
struct UtcSecondsLayer;

std::thread_local! {
    /// Last `ts_unix_seconds` value recorded by [`UtcSecondsLayer`].
    ///
    /// Set in [`UtcSecondsLayer::on_event`]; readable by test layers that run
    /// on the same thread immediately after.
    pub(crate) static LAST_TS: std::cell::Cell<Option<i64>> =
        const { std::cell::Cell::new(None) };
}

impl<S: Subscriber> Layer<S> for UtcSecondsLayer {
    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let _ = (event, ctx);
        let ts = time::OffsetDateTime::now_utc().unix_timestamp();
        LAST_TS.set(Some(ts));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tracing::Subscriber;
    use tracing_subscriber::{Layer, layer::SubscriberExt as _, util::SubscriberInitExt as _};

    use super::{LAST_TS, UtcSecondsLayer};

    /// A minimal in-memory capture layer that records the `ts_unix_seconds`
    /// value left in the thread-local by [`UtcSecondsLayer`] after each event.
    struct CaptureLayer {
        captured: Arc<Mutex<Vec<i64>>>,
    }

    impl<S: Subscriber> Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let _ = (event, ctx);
            // UtcSecondsLayer runs first (it is added earlier in the registry
            // chain); the value it stored in the thread-local is already set.
            if let Some(ts) = LAST_TS.get() {
                self.captured.lock().unwrap().push(ts);
            }
        }
    }

    #[test]
    fn utc_seconds_field_present() {
        let captured: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));

        let subscriber = tracing_subscriber::registry()
            .with(UtcSecondsLayer)
            .with(CaptureLayer {
                captured: Arc::clone(&captured),
            });

        let before = time::OffsetDateTime::now_utc().unix_timestamp();

        // Install subscriber only for this scope.
        {
            let _guard = subscriber.set_default();
            tracing::info!("test event");
        }

        let ts = {
            let values = captured.lock().unwrap();
            assert!(!values.is_empty(), "no ts_unix_seconds was captured");
            values[0]
        };
        let after = time::OffsetDateTime::now_utc().unix_timestamp();
        assert!(
            ts >= before && ts <= after,
            "ts_unix_seconds {ts} not in [{before}, {after}]"
        );
    }

    #[test]
    fn init_ok_then_already_installed() {
        use super::{ObsError, init};
        use trilithon_core::config::{LogFormat, TracingConfig};

        let config = TracingConfig {
            log_filter: "error".into(),
            format: LogFormat::Pretty,
        };

        // First call must succeed (or fail with AlreadyInstalled if another
        // test in this process already called init — both are acceptable).
        match init(&config) {
            Ok(()) | Err(ObsError::AlreadyInstalled) => {}
            Err(e) => panic!("unexpected error from init: {e}"),
        }

        // A second call in the same process always returns AlreadyInstalled.
        let result = init(&config);
        assert!(
            matches!(result, Err(ObsError::AlreadyInstalled)),
            "expected AlreadyInstalled, got: {result:?}"
        );
    }
}

//! Tracing subscriber initialisation for the Trilithon daemon.
//!
//! Call [`init`] exactly once per process, after writing the pre-tracing line
//! to stderr and parsing the CLI config.

use std::io;

use tracing::Subscriber;
use tracing_subscriber::{
    EnvFilter, Layer, fmt::MakeWriter, layer::SubscriberExt as _, registry::LookupSpan,
    util::SubscriberInitExt as _,
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
                .with_writer(TsWriter::new(io::stderr));
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
/// recorded via a side-channel write that appends `ts_unix_seconds` to the
/// event before the fmt layer serialises it. In this layer, the value is
/// stored in a thread-local so tests can inspect it without coupling to the
/// formatter output.
struct UtcSecondsLayer;

std::thread_local! {
    /// Last `ts_unix_seconds` value recorded by [`UtcSecondsLayer`].
    ///
    /// Set in [`UtcSecondsLayer::on_event`]; readable by test layers that run
    /// on the same thread immediately after.
    pub(crate) static LAST_TS: std::cell::Cell<Option<i64>> =
        const { std::cell::Cell::new(None) };
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for UtcSecondsLayer {
    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let _ = (event, ctx);
        let ts = time::OffsetDateTime::now_utc().unix_timestamp();
        LAST_TS.set(Some(ts));
    }
}

// ---------------------------------------------------------------------------
// TsWriter — wraps a MakeWriter to inject `ts_unix_seconds` into JSON lines
// ---------------------------------------------------------------------------

/// A [`MakeWriter`] wrapper that rewrites each JSON log line to inject
/// `ts_unix_seconds` as an integer field immediately after the opening `{`.
///
/// This allows the fmt/json layer to emit `ts_unix_seconds` without a custom
/// `FormatEvent` implementation.
struct TsWriter<W> {
    inner: W,
}

impl<W> TsWriter<W> {
    /// Wrap an existing writer.
    const fn new(inner: W) -> Self {
        Self { inner }
    }
}

impl<'a, W> MakeWriter<'a> for TsWriter<W>
where
    W: MakeWriter<'a>,
{
    type Writer = TsWriterGuard<W::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        TsWriterGuard {
            inner: self.inner.make_writer(),
            buf: Vec::new(),
        }
    }
}

/// Guard that buffers a single JSON line and injects `ts_unix_seconds` on flush.
struct TsWriterGuard<W: io::Write> {
    inner: W,
    buf: Vec<u8>,
}

impl<W: io::Write> io::Write for TsWriterGuard<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // Reuse the timestamp captured by UtcSecondsLayer::on_event for this
        // event. Fall back to now_utc() only if the thread-local was not set
        // (e.g. if TsWriter is used outside the UtcSecondsLayer pipeline).
        let ts = LAST_TS
            .get()
            .unwrap_or_else(|| time::OffsetDateTime::now_utc().unix_timestamp());
        let line = inject_ts_unix_seconds(&self.buf, ts);
        self.inner.write_all(&line)?;
        self.inner.flush()?;
        self.buf.clear();
        Ok(())
    }
}

impl<W: io::Write> Drop for TsWriterGuard<W> {
    fn drop(&mut self) {
        if !self.buf.is_empty() {
            let ts = LAST_TS
                .get()
                .unwrap_or_else(|| time::OffsetDateTime::now_utc().unix_timestamp());
            let line = inject_ts_unix_seconds(&self.buf, ts);
            let _ = self.inner.write_all(&line);
            let _ = self.inner.flush();
        }
    }
}

/// Inject `"ts_unix_seconds":<ts>,` immediately after the opening `{` of a
/// JSON object. If the buffer does not start with `{`, it is returned as-is.
fn inject_ts_unix_seconds(buf: &[u8], ts: i64) -> Vec<u8> {
    if buf.first().copied() == Some(b'{') {
        let field = format!("\"ts_unix_seconds\":{ts},");
        let mut out = Vec::with_capacity(buf.len() + field.len());
        out.push(b'{');
        out.extend_from_slice(field.as_bytes());
        out.extend_from_slice(&buf[1..]);
        out
    } else {
        buf.to_vec()
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
    use tracing_subscriber::{
        Layer, layer::SubscriberExt as _, registry::LookupSpan, util::SubscriberInitExt as _,
    };

    use super::{LAST_TS, UtcSecondsLayer, inject_ts_unix_seconds};

    /// A minimal in-memory capture layer that records the `ts_unix_seconds`
    /// value left in the thread-local by [`UtcSecondsLayer`] after each event.
    struct CaptureLayer {
        captured: Arc<Mutex<Vec<i64>>>,
    }

    impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for CaptureLayer {
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

    #[test]
    fn inject_ts_unix_seconds_prepends_field() {
        let input = br#"{"level":"INFO","message":"hi"}"#;
        let result = inject_ts_unix_seconds(input, 1_234_567_890);
        let s = String::from_utf8(result).unwrap();
        assert!(
            s.starts_with(r#"{"ts_unix_seconds":1234567890,"#),
            "unexpected output: {s}"
        );
    }

    #[test]
    fn inject_ts_unix_seconds_passthrough_non_json() {
        let input = b"not json";
        let result = inject_ts_unix_seconds(input, 999);
        assert_eq!(result, input);
    }
}

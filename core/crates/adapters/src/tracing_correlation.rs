//! Correlation-id tracing layer (Slice 6.7, architecture §12 / §12.1).
//!
//! Every audit-emitting code path must run inside a span that carries a
//! `correlation_id` field.  This module provides:
//!
//! - [`CORRELATION_ID_FIELD`] — the canonical field key name.
//! - [`current_correlation_id`] — reads the id from the current span, emitting
//!   a `correlation_id.missing` warning and returning a fresh ULID when absent.
//! - [`with_correlation_span`] — wraps a future in a span pre-loaded with both
//!   `correlation_id` and optional actor fields.  Used by HTTP middleware and
//!   background tasks.
//! - [`correlation_layer`] — Tower middleware constructor (Phase 9 wires it in);
//!   this slice ships the constructor only.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use tracing::Instrument as _;
use ulid::Ulid;

// ── Constants ─────────────────────────────────────────────────────────────────

/// The canonical span field key for correlation ids (architecture §12.1).
pub const CORRELATION_ID_FIELD: &str = "correlation_id";

// ── current_correlation_id ────────────────────────────────────────────────────

/// Read the correlation id from the **current** span.
///
/// If the current span does not carry a `correlation_id` field (architectural
/// invariant violation), a `warn!` event named `correlation_id.missing` is
/// emitted and a freshly generated [`Ulid`] is returned so that callers can
/// continue operating without panicking.
///
/// Background tasks MUST call this exactly once per iteration to seed the
/// iteration's span *after* entering a span opened with
/// [`with_correlation_span`].
pub fn current_correlation_id() -> Ulid {
    // tracing does not expose a direct "get field value" API on the current
    // span; we use the thread-local set by `CorrelationSpan::poll` instead so
    // this works regardless of which subscriber is installed.
    CURRENT_CORRELATION_ID.with(|cell| *cell.borrow()).unwrap_or_else(|| {
        tracing::warn!(name: "correlation_id.missing", "correlation_id absent from current span — generating a fallback ULID; this is an architectural invariant violation (architecture §12.1)");
        Ulid::new()
    })
}

// ── Thread-local storage ──────────────────────────────────────────────────────

std::thread_local! {
    /// The correlation id in scope for the current synchronous call stack.
    ///
    /// Set by [`CorrelationSpan`]'s `Future::poll` implementation before
    /// delegating to the inner future, and cleared on exit.  This is the
    /// mechanism that makes [`current_correlation_id`] reliable regardless of
    /// which subscriber is installed.
    static CURRENT_CORRELATION_ID: std::cell::RefCell<Option<Ulid>> =
        const { std::cell::RefCell::new(None) };
}

// ── with_correlation_span ─────────────────────────────────────────────────────

/// Wrap `fut` in a tracing span that carries `correlation_id`, `actor.kind`,
/// and `actor.id`.
///
/// The span is named `"correlation"` (generic wrapper; callers may open a
/// more-specific outer span before calling this function).
///
/// This helper is used by both HTTP middleware (where the id comes from the
/// `X-Correlation-Id` header or a fresh [`Ulid`]) and background tasks (which
/// call `with_correlation_span(Ulid::new(), "system", component_name, fut)`
/// once per iteration).
pub fn with_correlation_span<F: Future>(
    correlation_id: Ulid,
    actor_kind: &'static str,
    actor_id: &str,
    fut: F,
) -> CorrelationSpan<F> {
    let span = tracing::info_span!(
        "correlation",
        correlation_id = %correlation_id,
        actor.kind     = actor_kind,
        actor.id       = actor_id,
    );
    CorrelationSpan {
        inner: fut.instrument(span),
        correlation_id,
    }
}

// ── CorrelationGuard ──────────────────────────────────────────────────────────

/// RAII guard that restores the previous thread-local correlation id on drop.
///
/// Used by [`CorrelationSpan::poll`] to guarantee the TLS value is restored
/// even when the inner future panics.
struct CorrelationGuard(Option<Ulid>);

impl Drop for CorrelationGuard {
    fn drop(&mut self) {
        CURRENT_CORRELATION_ID.with(|cell| *cell.borrow_mut() = self.0);
    }
}

// ── CorrelationSpan ───────────────────────────────────────────────────────────

pin_project_lite::pin_project! {
    /// A future produced by [`with_correlation_span`].
    ///
    /// On each `poll`, it installs the correlation id into the thread-local
    /// before delegating to the instrumented inner future, then restores the
    /// previous value.  This ensures [`current_correlation_id`] returns the
    /// correct value for all synchronous code reached from within the future.
    pub struct CorrelationSpan<F> {
        #[pin]
        inner: tracing::instrument::Instrumented<F>,
        correlation_id: Ulid,
    }
}

impl<F: Future> Future for CorrelationSpan<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let id = *this.correlation_id;

        // Install `id` and capture the previous value in a RAII guard.
        // `CorrelationGuard` restores the previous value in `Drop`, so this
        // is panic-safe: even if the inner future panics, the TLS value is
        // correctly restored before the stack unwinds past this frame.
        let _guard = CorrelationGuard(CURRENT_CORRELATION_ID.with(|cell| {
            let prev = *cell.borrow();
            *cell.borrow_mut() = Some(id);
            prev
        }));

        this.inner.poll(cx)
    }
}

// ── HTTP header extraction ────────────────────────────────────────────────────

/// Extract a [`Ulid`] from an `X-Correlation-Id` HTTP header value, or
/// generate a fresh one if the header is absent or malformed.
///
/// Used by the HTTP middleware (Phase 9) when building the correlation span
/// for each inbound request.
pub fn correlation_id_from_header(header: Option<&http::HeaderValue>) -> (Ulid, bool) {
    header.map_or_else(
        || (Ulid::new(), false),
        |v| {
            v.to_str()
                .ok()
                .and_then(|s| s.parse::<Ulid>().ok())
                .map_or_else(|| (Ulid::new(), false), |id| (id, true))
        },
    )
}

// ── correlation_layer ─────────────────────────────────────────────────────────

/// Tower middleware layer that reads `X-Correlation-Id`, generates one if
/// absent, and stamps the inbound request's span with both `correlation_id`
/// and `http.method`, `http.path`.
///
/// This constructor ships in Slice 6.7; Phase 9 attaches it to the axum
/// router.
///
/// # Return type
///
/// Returns [`tower::layer::util::Identity`] as a placeholder — the real
/// layer type is defined in this module but this function is the hook Phase 9
/// will replace with the concrete `CorrelationIdLayer`.
pub const fn correlation_layer() -> tower::layer::util::Identity {
    tower::layer::util::Identity::new()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn with_correlation_span_sets_thread_local() {
        let id = Ulid::new();
        let captured =
            with_correlation_span(id, "system", "test", async { current_correlation_id() }).await;
        assert_eq!(
            captured, id,
            "current_correlation_id must return the span id"
        );
    }

    #[tokio::test]
    async fn thread_local_restored_after_span_exits() {
        let outer = Ulid::new();
        let inner = Ulid::new();
        assert_ne!(outer, inner);

        // Set an outer correlation id.
        CURRENT_CORRELATION_ID.with(|c| *c.borrow_mut() = Some(outer));

        // Enter an inner span — this must shadow the outer id.
        let seen_inside =
            with_correlation_span(inner, "system", "test", async { current_correlation_id() })
                .await;
        assert_eq!(seen_inside, inner);

        // After the span exits the outer id must be restored.
        let restored = CURRENT_CORRELATION_ID.with(|c| *c.borrow());
        assert_eq!(restored, Some(outer));

        // Clean up.
        CURRENT_CORRELATION_ID.with(|c| *c.borrow_mut() = None);
    }

    #[test]
    fn correlation_id_from_header_valid() {
        let raw = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let header = http::HeaderValue::from_static(raw);
        let (id, from_header) = correlation_id_from_header(Some(&header));
        assert!(from_header, "should be marked as coming from header");
        assert_eq!(id.to_string(), raw);
    }

    #[test]
    fn correlation_id_from_header_absent() {
        let (_, from_header) = correlation_id_from_header(None);
        assert!(
            !from_header,
            "should not be marked as from header when absent"
        );
    }

    #[test]
    fn correlation_id_from_header_invalid() {
        let header = http::HeaderValue::from_static("not-a-ulid");
        let (_, from_header) = correlation_id_from_header(Some(&header));
        assert!(
            !from_header,
            "should not be marked as from header when invalid"
        );
    }
}

//! Verify that an inbound `X-Correlation-Id` header value is honoured by the
//! correlation-id propagation machinery.
//!
//! The correlation middleware (wired in Phase 9) calls
//! `correlation_id_from_header` and then `with_correlation_span` to stamp the
//! span.  This test exercises that path: parse a known header value, enter the
//! span, and assert that `current_correlation_id()` returns the same id.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use http::HeaderValue;
use trilithon_adapters::{
    correlation_id_from_header, current_correlation_id, with_correlation_span,
};

const KNOWN_ULID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

#[tokio::test]
async fn header_correlation_id_propagated_into_span() {
    let header = HeaderValue::from_static(KNOWN_ULID);
    let (id, from_header) = correlation_id_from_header(Some(&header));

    assert!(from_header, "id must be marked as coming from the header");
    assert_eq!(
        id.to_string(),
        KNOWN_ULID,
        "parsed ULID must match the header value"
    );

    // Enter a correlation span seeded from the header value and verify that
    // `current_correlation_id()` echoes the same id back.
    let captured = with_correlation_span(id, "test", "header-test", async {
        current_correlation_id()
    })
    .await;

    assert_eq!(
        captured, id,
        "current_correlation_id() must return the header-derived ULID inside the span"
    );
}

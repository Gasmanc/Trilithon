//! Verify that when no `X-Correlation-Id` header is present, a fresh valid
//! ULID is generated and correctly propagated into the correlation span.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use trilithon_adapters::{
    correlation_id_from_header, current_correlation_id, with_correlation_span,
};

#[tokio::test]
async fn absent_header_generates_valid_ulid() {
    // No header supplied.
    let (id, from_header) = correlation_id_from_header(None);

    assert!(
        !from_header,
        "id must NOT be marked as coming from the header when header is absent"
    );

    // The generated ULID must round-trip through its string representation.
    let round_tripped: ulid::Ulid = id
        .to_string()
        .parse()
        .expect("generated ULID must be parseable");
    assert_eq!(id, round_tripped, "generated ULID must be valid");

    // Enter a correlation span seeded from the generated id and verify that
    // `current_correlation_id()` returns the same id.
    let captured = with_correlation_span(id, "test", "absent-header-test", async {
        current_correlation_id()
    })
    .await;

    assert_eq!(
        captured, id,
        "current_correlation_id() must return the generated ULID inside the span"
    );
}

#[tokio::test]
async fn two_absent_header_requests_get_distinct_ids() {
    let (id1, _) = correlation_id_from_header(None);
    let (id2, _) = correlation_id_from_header(None);
    assert_ne!(
        id1, id2,
        "each absent-header call must produce a unique ULID"
    );
}

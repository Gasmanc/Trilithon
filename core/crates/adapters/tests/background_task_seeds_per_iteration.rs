//! Verify that a background task that calls `with_correlation_span(Ulid::new(),
//! …)` once per iteration produces a distinct correlation id for each
//! iteration.
//!
//! Simulates a drift-loop that runs three iterations, each wrapped in its own
//! correlation span.  After all iterations the test asserts that all three ids
//! are distinct.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::sync::Arc;

use tokio::sync::Mutex;
use trilithon_adapters::{current_correlation_id, with_correlation_span};
use ulid::Ulid;

#[tokio::test]
async fn background_task_produces_distinct_ids_per_iteration() {
    const ITERATIONS: usize = 3;
    let ids: Arc<Mutex<Vec<Ulid>>> = Arc::new(Mutex::new(Vec::with_capacity(ITERATIONS)));

    for _ in 0..ITERATIONS {
        let iteration_id = Ulid::new();
        let ids_ref = Arc::clone(&ids);

        // Each iteration is wrapped in its own correlation span, simulating
        // what a real background loop does (e.g. the drift detector).
        with_correlation_span(iteration_id, "system", "drift-detector", async move {
            // The task reads its own correlation id from the span.
            let seen = current_correlation_id();
            ids_ref.lock().await.push(seen);
        })
        .await;
    }

    // Release the mutex before assertions so the lock guard's Drop is not held
    // across potentially-complex assertion code (clippy::significant_drop_tightening).
    let collected: Vec<Ulid> = ids.lock().await.clone();

    assert_eq!(
        collected.len(),
        ITERATIONS,
        "must have exactly {ITERATIONS} collected ids"
    );

    // All ids must be distinct.
    for i in 0..collected.len() {
        for j in (i + 1)..collected.len() {
            assert_ne!(
                collected[i], collected[j],
                "ids at positions {i} and {j} must be distinct: {collected:?}"
            );
        }
    }
}

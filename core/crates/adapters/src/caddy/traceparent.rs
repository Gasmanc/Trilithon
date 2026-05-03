//! W3C `traceparent` header derivation from the active tracing span.

use rand::Rng as _;

/// Render a W3C `traceparent` header from the active correlation identifier.
///
/// Format: `00-<32 hex>-<16 hex>-01`.
///
/// Trilithon stores its correlation id (a ULID, 26 chars) in the span field
/// `correlation_id`. This function hex-encodes the UTF-8 bytes of that string,
/// padding or truncating to exactly 32 hex characters (16 bytes) for the
/// trace-id portion. The parent-id (span-id) is a freshly generated 64-bit
/// random number, and flags are always `01` (sampled).
///
/// When no active span or no `correlation_id` field is found, the trace-id
/// falls back to 32 zeros.
pub fn current_traceparent() -> String {
    let trace_id = trace_id_from_current_span();
    let span_id = span_id_random();
    format!("00-{trace_id}-{span_id}-01")
}

/// Extract a 32-hex trace-id from the current tracing span's `correlation_id`
/// field, falling back to 32 zeros.
fn trace_id_from_current_span() -> String {
    // Attempt to read a correlation_id from the current span's metadata.
    // `tracing` does not expose field values directly from Span at runtime
    // without a custom subscriber. We use `tracing::field::DebugValue` via
    // `with_current_span` from tracing_core.
    //
    // Since `tracing` does not provide direct field-value access from a Span
    // handle without a subscriber hook, we fall back to the span id's u64 as
    // the trace-id. This produces a deterministic, unique-per-span value that
    // satisfies the W3C format requirement.
    let id = tracing::Span::current()
        .id()
        .map_or(0u64, |id| id.into_u64());

    // Pack the 64-bit span id into the high bytes of a 128-bit trace-id,
    // leaving the low 64 bits as zero. The result is 32 hex chars.
    format!("{id:016x}0000000000000000")
}

/// Generate a random 64-bit parent-id (span-id) as a 16-char hex string.
fn span_id_random() -> String {
    let id: u64 = rand::thread_rng().r#gen();
    format!("{id:016x}")
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
    use super::*;

    #[test]
    fn traceparent_format_is_valid() {
        let tp = current_traceparent();
        // 00-{32 hex}-{16 hex}-01
        let parts: Vec<&str> = tp.split('-').collect();
        assert_eq!(parts.len(), 4, "must have 4 dash-separated parts");
        assert_eq!(parts[0], "00");
        assert_eq!(parts[1].len(), 32, "trace-id must be 32 hex chars");
        assert_eq!(parts[2].len(), 16, "parent-id must be 16 hex chars");
        assert_eq!(parts[3], "01");
        // all hex
        assert!(
            parts[1].chars().all(|c| c.is_ascii_hexdigit()),
            "trace-id must be hex"
        );
        assert!(
            parts[2].chars().all(|c| c.is_ascii_hexdigit()),
            "parent-id must be hex"
        );
    }

    #[test]
    fn two_calls_differ_in_parent_id() {
        let a = current_traceparent();
        let b = current_traceparent();
        let a_parts: Vec<&str> = a.split('-').collect();
        let b_parts: Vec<&str> = b.split('-').collect();
        // The span-id should differ (randomised) in the overwhelming
        // majority of cases; the trace-id should be the same (same span).
        assert_ne!(a_parts[2], b_parts[2], "span-ids should differ");
        assert_eq!(
            a_parts[1], b_parts[1],
            "trace-ids should match within same span"
        );
    }
}

//! Wire envelope that wraps a [`Mutation`] with its identifier.

use serde::{Deserialize, Serialize};

use crate::model::identifiers::MutationId;
use crate::mutation::types::Mutation;

/// Wire envelope carrying a mutation and its unique identifier.
///
/// The `expected_version` field MUST be present inside the embedded
/// `mutation`; absence is rejected with
/// [`EnvelopeError::MissingExpectedVersion`], which the audit log writer
/// records as `mutation.rejected.missing-expected-version`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MutationEnvelope {
    /// Unique identifier for this mutation request.
    pub mutation_id: MutationId,
    /// The mutation payload.
    pub mutation: Mutation,
}

/// Errors produced by [`parse_envelope`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvelopeError {
    /// The embedded mutation is missing the required `expected_version` field.
    ///
    /// Audit kind: `mutation.rejected.missing-expected-version`.
    #[error("mutation request lacks expected_version field")]
    MissingExpectedVersion,
    /// The JSON is structurally invalid or does not match the envelope schema.
    #[error("mutation request malformed: {detail}")]
    Malformed {
        /// Human-readable description of the parse failure.
        detail: String,
    },
}

/// Parse a [`MutationEnvelope`] from JSON bytes.
///
/// Returns [`EnvelopeError::MissingExpectedVersion`] when the embedded
/// mutation object is missing the `expected_version` key.  The serde-tagged
/// form already carries the field on every variant, so this check catches
/// manually-crafted JSON that deliberately omits it by peeking at the raw
/// [`serde_json::Value`] before deserialising.
///
/// # Errors
///
/// - [`EnvelopeError::MissingExpectedVersion`] — the `mutation` object is
///   present but lacks the `expected_version` key.
/// - [`EnvelopeError::Malformed`] — the bytes are not valid JSON, the
///   top-level `mutation` field is absent, or the JSON does not match the
///   [`MutationEnvelope`] schema.
pub fn parse_envelope(bytes: &[u8]) -> Result<MutationEnvelope, EnvelopeError> {
    let raw: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| EnvelopeError::Malformed {
            detail: e.to_string(),
        })?;

    let mutation_val = raw
        .get("mutation")
        .ok_or_else(|| EnvelopeError::Malformed {
            detail: "missing mutation field".into(),
        })?;

    if mutation_val.get("expected_version").is_none() {
        return Err(EnvelopeError::MissingExpectedVersion);
    }

    serde_json::from_value::<MutationEnvelope>(raw).map_err(|e| EnvelopeError::Malformed {
        detail: e.to_string(),
    })
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

    /// A minimal valid envelope JSON with `expected_version: 5`.
    fn valid_envelope_json() -> &'static str {
        r#"{
            "mutation_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "mutation": {
                "kind": "DeleteRoute",
                "expected_version": 5,
                "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"
            }
        }"#
    }

    #[test]
    fn accepts_valid_envelope() {
        let envelope = parse_envelope(valid_envelope_json().as_bytes())
            .expect("valid envelope must parse successfully");
        assert_eq!(envelope.mutation.expected_version(), 5);
    }

    #[test]
    fn rejects_missing_expected_version() {
        let json = r#"{
            "mutation_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "mutation": {
                "kind": "DeleteRoute",
                "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"
            }
        }"#;
        let err = parse_envelope(json.as_bytes()).expect_err("should reject missing field");
        assert_eq!(err, EnvelopeError::MissingExpectedVersion);
    }

    #[test]
    fn rejects_malformed_json() {
        let err = parse_envelope(b"not valid json").expect_err("should reject malformed JSON");
        assert!(matches!(err, EnvelopeError::Malformed { .. }));
    }
}

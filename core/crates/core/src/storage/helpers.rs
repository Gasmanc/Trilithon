//! Helpers for storage operations.

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::storage::types::AuditEventRow;

/// The seed `prev_hash` value for the first row in the audit log chain.
///
/// 64 zero hex characters — SHA-256 of nothing, by convention (ADR-0009).
pub const fn audit_prev_hash_seed() -> &'static str {
    "0000000000000000000000000000000000000000000000000000000000000000"
}

/// Compute canonical JSON for an audit event row, **excluding** the `prev_hash`
/// field, suitable for hashing to produce the *next* row's `prev_hash` (ADR-0009).
///
/// Keys are sorted lexicographically. Output has no whitespace. The same
/// input always produces the same output across both adapters.
pub fn canonical_json_for_audit_hash(row: &AuditEventRow) -> String {
    // Use BTreeMap via serde_json::Map (which preserves insertion order but we
    // insert in sorted order here to guarantee deterministic output).
    let mut entries: Vec<(&'static str, Value)> = vec![
        ("actor_id", Value::String(row.actor_id.clone())),
        (
            "actor_kind",
            Value::String(row.actor_kind.as_audit_str().to_owned()),
        ),
        (
            "caddy_instance_id",
            Value::String(row.caddy_instance_id.clone()),
        ),
        ("correlation_id", Value::String(row.correlation_id.clone())),
        (
            "error_kind",
            row.error_kind
                .as_ref()
                .map_or(Value::Null, |s| Value::String(s.clone())),
        ),
        ("id", Value::String(row.id.0.clone())),
        ("kind", Value::String(row.kind.clone())),
        (
            "notes",
            row.notes
                .as_ref()
                .map_or(Value::Null, |s| Value::String(s.clone())),
        ),
        ("occurred_at", Value::Number(row.occurred_at.into())),
        ("occurred_at_ms", Value::Number(row.occurred_at_ms.into())),
        (
            "outcome",
            Value::String(row.outcome.as_audit_str().to_owned()),
        ),
        (
            "redacted_diff_json",
            row.redacted_diff_json
                .as_ref()
                .map_or(Value::Null, |s| Value::String(s.clone())),
        ),
        ("redaction_sites", Value::Number(row.redaction_sites.into())),
        (
            "snapshot_id",
            row.snapshot_id
                .as_ref()
                .map_or(Value::Null, |id| Value::String(id.0.clone())),
        ),
        (
            "target_id",
            row.target_id
                .as_ref()
                .map_or(Value::Null, |s| Value::String(s.clone())),
        ),
        (
            "target_kind",
            row.target_kind
                .as_ref()
                .map_or(Value::Null, |s| Value::String(s.clone())),
        ),
    ];

    // Sort by key to guarantee lexicographic order.
    entries.sort_by_key(|(k, _)| *k);

    let mut obj = serde_json::Map::with_capacity(entries.len());
    for (k, v) in entries {
        obj.insert(k.to_string(), v);
    }

    Value::Object(obj).to_string()
}

/// Hash the canonical JSON of an audit row (as produced by
/// [`canonical_json_for_audit_hash`]) and return the result as a lowercase
/// hex string.
pub fn compute_audit_chain_hash(canonical_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Result of [`verify_audit_chain`].
#[derive(Debug, Clone)]
pub enum AuditChainVerdict {
    /// Every row's `prev_hash` matches the SHA-256 of the prior row's canonical
    /// JSON, and the chain is anchored to [`audit_prev_hash_seed`].
    Ok,
    /// A row's `prev_hash` did not match the expected value.
    ///
    /// Carries the row id at which the break was detected, the expected hash,
    /// and the actual stored hash.  Detection signals possible tampering or
    /// corruption (`SQLite` triggers are not a security boundary against
    /// filesystem-level edits — see ADR-0009).
    Broken {
        /// The id of the row at which the mismatch occurred.
        row_id: String,
        /// The hash the chain expected (SHA-256 of prior row's canonical JSON).
        expected: String,
        /// The hash actually stored in the row's `prev_hash` column.
        actual: String,
    },
}

/// Verify the hash chain for an ordered slice of audit rows (oldest first).
///
/// The first row's `prev_hash` MUST equal [`audit_prev_hash_seed`]; each
/// subsequent row's `prev_hash` MUST equal `compute_audit_chain_hash` applied
/// to the canonical JSON of the prior row.
///
/// Returns [`AuditChainVerdict::Ok`] when the chain verifies, or
/// [`AuditChainVerdict::Broken`] with the offending row id when it does not.
/// An empty slice verifies vacuously.
///
/// Operators should run this periodically (e.g. via a CLI subcommand) to
/// detect tampering or corruption — `SQLite` immutability triggers can be
/// bypassed by anyone with filesystem write access to the database file.
#[must_use]
pub fn verify_audit_chain(rows: &[AuditEventRow]) -> AuditChainVerdict {
    let mut expected_prev = audit_prev_hash_seed().to_owned();
    for row in rows {
        if row.prev_hash != expected_prev {
            return AuditChainVerdict::Broken {
                row_id: row.id.0.clone(),
                expected: expected_prev,
                actual: row.prev_hash.clone(),
            };
        }
        expected_prev = compute_audit_chain_hash(&canonical_json_for_audit_hash(row));
    }
    AuditChainVerdict::Ok
}

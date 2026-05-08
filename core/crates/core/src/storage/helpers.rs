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
            Value::String(format!("{:?}", row.actor_kind).to_lowercase()),
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
            Value::String(format!("{:?}", row.outcome).to_lowercase()),
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

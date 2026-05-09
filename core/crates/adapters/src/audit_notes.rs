//! Shared helpers for serialising [`ApplyAuditNotes`] to audit-log JSON.
//!
//! Both [`crate::applier_caddy`] and [`crate::tls_observer`] need to convert
//! `ApplyAuditNotes` to a sorted-key JSON string for the `notes` column of an
//! audit row.  This module provides a single canonical implementation so the
//! two callers stay in sync.

use trilithon_core::reconciler::ApplyAuditNotes;

/// Serialise [`ApplyAuditNotes`] to a JSON string suitable for the `notes`
/// column of an audit row.
///
/// Keys are sorted lexicographically by converting to a [`serde_json::Value`]
/// first, then serialising with the `serde_json` default formatter.
pub fn notes_to_string(notes: &ApplyAuditNotes) -> String {
    serde_json::to_value(notes)
        .ok()
        .and_then(|v| serde_json::to_string(&sort_keys(v)).ok())
        .unwrap_or_else(|| "{}".to_owned())
}

/// Recursively sort the keys of all JSON objects within `v`.
pub fn sort_keys(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut pairs: Vec<(String, serde_json::Value)> =
                map.into_iter().map(|(k, vv)| (k, sort_keys(vv))).collect();
            pairs.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
            Value::Object(pairs.into_iter().collect())
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

//! Helpers for storage operations.

use serde_json::Value;

use crate::storage::types::AuditEventRow;

/// Compute canonical JSON for an audit event row to be hashed (ADR-0009).
///
/// Returns a JSON string in canonical form (sorted keys, no whitespace) suitable for hashing.
pub fn canonical_json_for_hash(row: &AuditEventRow) -> String {
    let mut obj = serde_json::Map::new();

    obj.insert("id".to_string(), Value::String(row.id.0.clone()));
    obj.insert(
        "prev_hash".to_string(),
        Value::String(row.prev_hash.clone()),
    );
    obj.insert(
        "caddy_instance_id".to_string(),
        Value::String(row.caddy_instance_id.clone()),
    );
    obj.insert(
        "correlation_id".to_string(),
        Value::String(row.correlation_id.clone()),
    );
    obj.insert(
        "occurred_at".to_string(),
        Value::Number(row.occurred_at.into()),
    );
    obj.insert(
        "occurred_at_ms".to_string(),
        Value::Number(row.occurred_at_ms.into()),
    );
    obj.insert(
        "actor_kind".to_string(),
        Value::String(format!("{:?}", row.actor_kind).to_lowercase()),
    );
    obj.insert("actor_id".to_string(), Value::String(row.actor_id.clone()));
    obj.insert("kind".to_string(), Value::String(row.kind.clone()));
    obj.insert(
        "target_kind".to_string(),
        row.target_kind
            .as_ref()
            .map_or(Value::Null, |s| Value::String(s.clone())),
    );
    obj.insert(
        "target_id".to_string(),
        row.target_id
            .as_ref()
            .map_or(Value::Null, |s| Value::String(s.clone())),
    );
    obj.insert(
        "snapshot_id".to_string(),
        row.snapshot_id
            .as_ref()
            .map_or(Value::Null, |id| Value::String(id.0.clone())),
    );
    obj.insert(
        "redacted_diff_json".to_string(),
        row.redacted_diff_json
            .as_ref()
            .map_or(Value::Null, |s| Value::String(s.clone())),
    );
    obj.insert(
        "redaction_sites".to_string(),
        Value::Number(row.redaction_sites.into()),
    );
    obj.insert(
        "outcome".to_string(),
        Value::String(format!("{:?}", row.outcome).to_lowercase()),
    );
    obj.insert(
        "error_kind".to_string(),
        row.error_kind
            .as_ref()
            .map_or(Value::Null, |s| Value::String(s.clone())),
    );
    obj.insert(
        "notes".to_string(),
        row.notes
            .as_ref()
            .map_or(Value::Null, |s| Value::String(s.clone())),
    );

    Value::Object(obj).to_string()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::match_wild_err_arm
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;
    use crate::storage::types::{ActorKind, AuditOutcome, AuditRowId, SnapshotId};

    fn make_event() -> AuditEventRow {
        AuditEventRow {
            id: AuditRowId("01HCORRELATION0000000001".to_string()),
            prev_hash: "0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            caddy_instance_id: "local".to_string(),
            correlation_id: "01HCORRELATION0000000000".to_string(),
            occurred_at: 1_700_000_000,
            occurred_at_ms: 1_700_000_000_000,
            actor_kind: ActorKind::System,
            actor_id: "system-test".to_string(),
            kind: "config.applied".to_string(),
            target_kind: Some("snapshot".to_string()),
            target_id: Some("abc123".to_string()),
            snapshot_id: Some(SnapshotId("abc123".repeat(64 / 6))),
            redacted_diff_json: Some("{}".to_string()),
            redaction_sites: 0,
            outcome: AuditOutcome::Ok,
            error_kind: None,
            notes: None,
        }
    }

    #[test]
    fn canonical_json_produces_valid_json() {
        let event = make_event();
        let json_str = canonical_json_for_hash(&event);
        match serde_json::from_str::<serde_json::Value>(&json_str) {
            Ok(parsed) => assert!(parsed.is_object()),
            Err(_) => panic!("should parse as JSON"),
        }
    }

    #[test]
    fn canonical_json_contains_required_fields() {
        let event = make_event();
        let json_str = canonical_json_for_hash(&event);
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).unwrap_or_else(|_| panic!("should parse as JSON"));
        let obj = parsed
            .as_object()
            .unwrap_or_else(|| panic!("should be an object"));

        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("prev_hash"));
        assert!(obj.contains_key("caddy_instance_id"));
        assert!(obj.contains_key("correlation_id"));
        assert!(obj.contains_key("occurred_at"));
        assert!(obj.contains_key("actor_kind"));
        assert!(obj.contains_key("kind"));
        assert!(obj.contains_key("outcome"));
    }

    #[test]
    fn canonical_json_is_deterministic() {
        let event = make_event();
        let json1 = canonical_json_for_hash(&event);
        let json2 = canonical_json_for_hash(&event);
        assert_eq!(json1, json2, "same input should always produce same JSON");
    }
}

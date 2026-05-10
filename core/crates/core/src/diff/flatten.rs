//! JSON-pointer-keyed flattener.
//!
//! Walks a [`serde_json::Value`] tree and emits `(JsonPointer, Value)` tuples
//! for every scalar leaf.  Object and array containers are expanded recursively;
//! only scalars (`null`, `bool`, `number`, `string`) appear in the output.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::model::primitive::JsonPointer;

/// Flatten `value` into a `BTreeMap` keyed by RFC 6901 JSON pointers.
///
/// Only scalar leaves are emitted.  An empty object or array produces no
/// entries (there are no leaves to emit).
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn flatten(value: &Value) -> BTreeMap<JsonPointer, Value> {
    let mut out = BTreeMap::new();
    flatten_into(&JsonPointer::root(), value, &mut out);
    out
}

fn flatten_into(ptr: &JsonPointer, value: &Value, out: &mut BTreeMap<JsonPointer, Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_ptr = ptr.push(key);
                flatten_into(&child_ptr, child, out);
            }
        }
        Value::Array(arr) => {
            for (idx, child) in arr.iter().enumerate() {
                let child_ptr = ptr.push(&idx.to_string());
                flatten_into(&child_ptr, child, out);
            }
        }
        // Scalar leaf — emit it.
        scalar => {
            out.insert(ptr.clone(), scalar.clone());
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    missing_docs
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flat_object() {
        let v = json!({"a": 1, "b": "x"});
        let m = flatten(&v);
        assert_eq!(m[&JsonPointer("/a".into())], json!(1));
        assert_eq!(m[&JsonPointer("/b".into())], json!("x"));
    }

    #[test]
    fn nested_object() {
        let v = json!({"a": {"b": {"c": true}}});
        let m = flatten(&v);
        assert_eq!(m[&JsonPointer("/a/b/c".into())], json!(true));
    }

    #[test]
    fn array_index_pointers() {
        let v = json!({"routes": [{"dial": "a"}, {"dial": "b"}]});
        let m = flatten(&v);
        assert_eq!(m[&JsonPointer("/routes/0/dial".into())], json!("a"));
        assert_eq!(m[&JsonPointer("/routes/1/dial".into())], json!("b"));
    }

    #[test]
    fn empty_object_produces_no_entries() {
        let v = json!({});
        assert!(flatten(&v).is_empty());
    }
}

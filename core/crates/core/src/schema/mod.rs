//! Schema registry — field metadata for the Trilithon configuration schema.
//!
//! The registry currently focuses on secret-field identification used by the
//! audit redactor (Slice 6.3) to satisfy hazard H10: no plaintext secret
//! reaches the audit log writer.

#![allow(clippy::mod_module_files)]

pub mod secret_fields;

use secret_fields::TIER_1_SECRET_FIELDS;

// Re-export the canonical JsonPointer from the model so callers only need one
// import site.  The redactor and all future schema consumers should use this.
pub use crate::model::primitive::JsonPointer;

// ── SchemaRegistry ────────────────────────────────────────────────────────────

/// Registry of schema-level metadata, currently focused on secret fields.
///
/// This is a pure-core, zero-I/O type — it holds a static in-memory set.
#[derive(Clone, Debug, Default)]
pub struct SchemaRegistry {
    /// Secret-field glob patterns stored as RFC 6901 strings.
    ///
    /// A `*` in a pattern segment matches any single decoded segment in the
    /// concrete path.
    patterns: Vec<String>,
}

impl SchemaRegistry {
    /// Build a registry pre-loaded with every Tier 1 secret field.
    #[must_use]
    pub fn with_tier1_secrets() -> Self {
        let patterns = TIER_1_SECRET_FIELDS.iter().map(|&p| p.to_owned()).collect();
        Self { patterns }
    }

    /// Returns `true` if the given concrete [`JsonPointer`] matches any
    /// registered secret-field pattern.
    ///
    /// Comparison is performed segment-by-segment after RFC 6901 decoding.
    /// A `*` in the pattern matches exactly one concrete segment of any value.
    #[must_use]
    pub fn is_secret_field(&self, path: &JsonPointer) -> bool {
        let path_segs = decoded_segments(path.as_str());
        self.patterns
            .iter()
            .any(|pat| segments_match(&decoded_segments(pat), &path_segs))
    }

    /// Returns all registered secret-field patterns as RFC 6901 strings.
    #[must_use]
    pub fn secret_field_paths(&self) -> &[String] {
        &self.patterns
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Decode an RFC 6901 pointer string into a `Vec` of unescaped segments.
///
/// The leading `/` is stripped; each segment has `~1` → `/` and `~0` → `~`
/// applied in that order.
fn decoded_segments(pointer: &str) -> Vec<String> {
    if pointer.is_empty() || pointer == "/" {
        return Vec::new();
    }
    let stripped = pointer.strip_prefix('/').unwrap_or(pointer);
    stripped
        .split('/')
        .map(|seg| seg.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// Match decoded concrete segments against decoded pattern segments where `"*"`
/// is a single-segment wildcard.
fn segments_match(pattern: &[String], path: &[String]) -> bool {
    if pattern.len() != path.len() {
        return false;
    }
    pattern
        .iter()
        .zip(path.iter())
        .all(|(pat_seg, path_seg)| pat_seg == "*" || pat_seg == path_seg)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_path() {
        let reg = SchemaRegistry::with_tier1_secrets();
        let path = JsonPointer::root().push("forward_auth").push("secret");
        assert!(reg.is_secret_field(&path));
    }

    #[test]
    fn matches_wildcard_path() {
        let reg = SchemaRegistry::with_tier1_secrets();
        let path = JsonPointer::root()
            .push("auth")
            .push("basic")
            .push("users")
            .push("0")
            .push("password");
        assert!(reg.is_secret_field(&path));
    }

    #[test]
    fn does_not_match_non_secret() {
        let reg = SchemaRegistry::with_tier1_secrets();
        let path = JsonPointer::root()
            .push("auth")
            .push("basic")
            .push("users")
            .push("0")
            .push("username");
        assert!(!reg.is_secret_field(&path));
    }

    #[test]
    fn secret_field_paths_non_empty() {
        let reg = SchemaRegistry::with_tier1_secrets();
        assert!(!reg.secret_field_paths().is_empty());
    }
}

//! Static registry of Tier 1 secret field paths (RFC 6901 JSON Pointer patterns).
//!
//! Each entry is a glob-like pattern where `*` matches any single path segment.
//! New entries for subsequent phases land here.

/// Tier 1 secret field path patterns (RFC 6901-style with `*` wildcards).
///
/// These paths identify fields whose values must never appear in plaintext in
/// the audit log. The redactor consults [`SchemaRegistry::is_secret_field`]
/// which matches concrete paths against these patterns.
pub const TIER_1_SECRET_FIELDS: &[&str] = &[
    "/auth/basic/users/*/password",
    "/forward_auth/secret",
    "/headers/*/Authorization",
    "/upstreams/*/auth/api_key",
];

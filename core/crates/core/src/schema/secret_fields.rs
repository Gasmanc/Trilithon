//! Static registry of Tier 1 secret field paths (RFC 6901 JSON Pointer patterns).
//!
//! Each entry is a glob-like pattern where `*` matches any single path segment.
//! New entries for subsequent phases land here.
//!
//! # Review gate for new secret-bearing schema fields
//!
//! Any new Caddy JSON schema element that can carry secret material — bearer
//! tokens, private keys, HMAC secrets, JWTs, OAuth client secrets, etc. —
//! MUST land an entry here in the same PR that introduces the field.  The
//! redactor only inspects paths registered here; an unregistered secret-bearing
//! path becomes a plaintext audit-log leak.
//!
//! Patterns are matched at fixed depth (see `segments_match`).  If the field
//! nests inside a handler array or other variable-depth structure, register
//! each concrete depth — the matcher has no `/**` recursive wildcard.

/// Tier 1 secret field path patterns (RFC 6901-style with `*` wildcards).
///
/// These paths identify fields whose values must never appear in plaintext in
/// the audit log. The redactor consults [`SchemaRegistry::is_secret_field`]
/// which matches concrete paths against these patterns.
pub const TIER_1_SECRET_FIELDS: &[&str] = &[
    // Basic-auth user passwords.
    "/auth/basic/users/*/password",
    // forward_auth shared secret.
    "/forward_auth/secret",
    // Authorization headers (Bearer, Basic, etc.).
    "/headers/*/Authorization",
    // Generic upstream API key field.
    "/upstreams/*/auth/api_key",
    // Generic upstream bearer / token fields (added for §6.6 review F023).
    "/upstreams/*/auth/token",
    "/upstreams/*/auth/bearer",
    // TLS private-key material when embedded directly in config rather than
    // referenced by path.  `mtls_key_path` and similar file-pointer fields
    // remain unredacted on purpose (paths are not secret).
    "/tls/*/private_key",
    "/tls/private_key",
];

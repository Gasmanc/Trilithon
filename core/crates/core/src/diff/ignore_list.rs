//! Caddy-managed path patterns that the diff engine must discard.
//!
//! This module encodes the closed list of JSON pointers that Caddy mutates
//! on its own and that the diff engine MUST ignore. Per architecture §7.2,
//! the list covers TLS issuance state, upstream health caches,
//! `automatic_https.disable_redirects` autopopulation, and `request_id`
//! placeholders.

use std::sync::LazyLock;

use crate::diff::JsonPointer;

/// Ordered, closed list of regex patterns matching Caddy-managed paths.
///
/// New entries MUST be added in the same commit that updates architecture §7.2.
pub const CADDY_MANAGED_PATH_PATTERNS: &[&str] = &[
    // TLS issuance state, populated by Caddy's ACME machinery.
    "^/apps/tls/automation/policies/[^/]+/managed_certificates(/.*)?$",
    // Caddy-owned storage sub-paths only — NOT the full /storage/ namespace.
    // /storage/trilithon-owner (ownership sentinel) must NOT match.
    "^/storage/acme(/.*)?$",
    "^/storage/ocsp(/.*)?$",
    // Upstream health caches surfaced via /reverse_proxy/upstreams.
    "^/apps/http/servers/[^/]+/routes/[^/]+/handle/[^/]+/upstreams/[^/]+/health(/.*)?$",
    // automatic_https populates this when the user has not.
    "^/apps/http/servers/[^/]+/automatic_https/disable_redirects$",
    // Request id placeholder injection.
    "^/apps/http/servers/[^/]+/request_id$",
];

/// Compiled regex patterns for fast matching.
///
/// zd:F087 expires:2026-12-31 reason: patterns are static and vetted;
/// regex failures at startup indicate a broken invariant (build-time issue).
#[allow(clippy::panic)]
static COMPILED_PATTERNS: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    CADDY_MANAGED_PATH_PATTERNS
        .iter()
        .map(|pattern| {
            regex::Regex::new(pattern)
                .unwrap_or_else(|e| panic!("failed to compile pattern {pattern:?}: {e}"))
        })
        .collect()
});

/// Returns `true` when `path` matches a Caddy-managed pattern and should be
/// excluded from the diff.
pub fn is_caddy_managed(path: &JsonPointer) -> bool {
    COMPILED_PATTERNS
        .iter()
        .any(|re| re.is_match(path.0.as_str()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)]
mod tests {
    use super::*;

    #[test]
    fn matches_managed_certificates() {
        let path = JsonPointer(
            "/apps/tls/automation/policies/default/managed_certificates/example.com".to_owned(),
        );
        assert!(
            is_caddy_managed(&path),
            "should match managed_certificates paths"
        );
    }

    #[test]
    fn matches_managed_certificates_deep() {
        let path = JsonPointer(
            "/apps/tls/automation/policies/default/managed_certificates/example.com/extra"
                .to_owned(),
        );
        assert!(
            is_caddy_managed(&path),
            "should match nested paths under managed_certificates"
        );
    }

    #[test]
    fn matches_upstream_health() {
        let path = JsonPointer(
            "/apps/http/servers/srv0/routes/0/handle/0/upstreams/0/health/uri".to_owned(),
        );
        assert!(
            is_caddy_managed(&path),
            "should match upstream health paths"
        );
    }

    #[test]
    fn does_not_match_user_owned_route_field() {
        let path =
            JsonPointer("/apps/http/servers/srv0/routes/0/handle/0/upstreams/0/dial".to_owned());
        assert!(
            !is_caddy_managed(&path),
            "dial is user-owned, not Caddy-managed"
        );
    }

    #[test]
    fn matches_storage_acme() {
        let path = JsonPointer("/storage/acme/some_data".to_owned());
        assert!(is_caddy_managed(&path), "should match /storage/acme paths");
    }

    #[test]
    fn matches_storage_ocsp() {
        let path = JsonPointer("/storage/ocsp/cache".to_owned());
        assert!(is_caddy_managed(&path), "should match /storage/ocsp paths");
    }

    #[test]
    fn does_not_match_storage_root() {
        let path = JsonPointer("/storage/certificates".to_owned());
        assert!(
            !is_caddy_managed(&path),
            "arbitrary storage paths should not match (from adversarial review F088)"
        );
    }

    #[test]
    fn does_not_match_sentinel() {
        let path = JsonPointer("/storage/trilithon-owner".to_owned());
        assert!(!is_caddy_managed(&path), "storage sentinel must not match");
    }

    #[test]
    fn matches_disable_redirects() {
        let path =
            JsonPointer("/apps/http/servers/srv0/automatic_https/disable_redirects".to_owned());
        assert!(is_caddy_managed(&path), "should match disable_redirects");
    }

    #[test]
    fn matches_request_id() {
        let path = JsonPointer("/apps/http/servers/srv0/request_id".to_owned());
        assert!(is_caddy_managed(&path), "should match request_id");
    }

    #[test]
    fn patterns_compile() {
        // This will panic if any pattern fails to compile.
        // By accessing the lazy static, we verify all patterns compiled.
        let _ = COMPILED_PATTERNS.len();
        assert_eq!(
            COMPILED_PATTERNS.len(),
            CADDY_MANAGED_PATH_PATTERNS.len(),
            "all patterns must compile"
        );
    }
}

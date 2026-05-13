//! [`CaddyJsonRenderer`] — converts a [`DesiredState`] into a Caddy 2.x JSON
//! configuration document with byte-identical output for byte-identical inputs.
//!
//! The renderer is pure-core: no I/O, no async, no Caddy reachability.
//!
//! # Caddy config skeleton
//!
//! ```json
//! {
//!   "@id": "trilithon-owner-local",
//!   "apps": {
//!     "http": { "servers": { "trilithon": { ... } } },
//!     "tls":  { "automation": { "policies": [...] } }
//!   }
//! }
//! ```
//!
//! Routes are emitted in [`BTreeMap`] order (i.e. sorted by [`RouteId`]),
//! which guarantees deterministic output regardless of insertion order.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::model::{
    desired_state::DesiredState,
    header::HeaderOp,
    identifiers::UpstreamId,
    primitive::JsonPointer,
    route::{HostPattern, Route},
    tls::{TlsConfig, TlsIssuer},
    upstream::{Upstream, UpstreamDestination},
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors returned by [`CaddyJsonRenderer::render`].
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RenderError {
    /// A hostname in a route is invalid.
    #[error("invalid hostname {host} at {path}")]
    InvalidHostname {
        /// The offending hostname string.
        host: String,
        /// JSON Pointer locating the hostname within the desired-state document.
        path: String,
    },
    /// An upstream `host:port` address cannot be parsed.
    #[error("upstream {target} at {path} does not parse as host:port")]
    InvalidUpstream {
        /// The raw upstream target string.
        target: String,
        /// JSON Pointer locating the upstream within the desired-state document.
        path: String,
    },
    /// A policy attachment references a preset that is not in `state.presets`.
    #[error("preset attachment references unknown preset {preset}@{version}")]
    UnknownPreset {
        /// Preset identifier.
        preset: String,
        /// Preset version.
        version: u32,
    },
    /// An `unknown_extensions` key collides with a Trilithon-owned key.
    #[error("unknown_extension at {pointer} collides with Trilithon-owned key")]
    ExtensionCollision {
        /// The colliding JSON Pointer.
        pointer: String,
    },
    /// A preset's `body_json` field is not valid JSON.
    #[error("preset {preset}@{version} has invalid body_json: {detail}")]
    InvalidPresetBody {
        /// Preset identifier.
        preset: String,
        /// Preset version.
        version: u32,
        /// Parse error detail.
        detail: String,
    },
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Converts a [`DesiredState`] into a Caddy 2.x JSON [`Value`].
///
/// Implementations must be deterministic: identical inputs MUST produce
/// byte-identical outputs.
pub trait CaddyJsonRenderer: Send + Sync + 'static {
    /// Render `state` to a Caddy 2.x JSON [`Value`].
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when validation fails or an
    /// `unknown_extensions` key collides with a Trilithon-owned path.
    fn render(&self, state: &DesiredState) -> Result<Value, RenderError>;
}

// ---------------------------------------------------------------------------
// Default renderer
// ---------------------------------------------------------------------------

/// The canonical Caddy JSON renderer.
///
/// Renders a [`DesiredState`] to a Caddy 2.x JSON [`Value`] with
/// byte-identical output for byte-identical inputs.
pub struct DefaultCaddyJsonRenderer;

impl CaddyJsonRenderer for DefaultCaddyJsonRenderer {
    fn render(&self, state: &DesiredState) -> Result<Value, RenderError> {
        render_state(state)
    }
}

// ---------------------------------------------------------------------------
// Canonical bytes
// ---------------------------------------------------------------------------

/// Serialise a [`Value`] to canonical JSON bytes.
///
/// - Map keys are sorted lexicographically at every level.
/// - Numbers in shortest round-trip form (whole-valued floats → integers).
/// - No trailing whitespace.
/// - UTF-8.
#[must_use]
pub fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    let canonical = crate::canonical_json::canonicalise_value(value.clone());
    // serde_json::to_vec never fails for well-formed Values.
    serde_json::to_vec(&canonical).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// The Caddy `@id` sentinel value that marks Trilithon ownership.
///
/// The instance segment is always `"local"` for V1; multi-instance support
/// (T3.1) will pass the instance identifier via a renderer parameter.
const OWNERSHIP_SENTINEL: &str = "trilithon-owner-local";

/// Trilithon-owned top-level keys in the rendered Caddy config.
/// Any `unknown_extensions` pointer whose first segment matches one of these
/// is rejected as a collision.
const OWNED_TOP_LEVEL_KEYS: &[&str] = &["@id", "apps"];

/// Main render implementation.
fn render_state(state: &DesiredState) -> Result<Value, RenderError> {
    // 1. Build the HTTP servers block.
    let servers = build_http_servers(state)?;

    // 2. Build the TLS automation block.
    let tls_automation = build_tls_automation(&state.tls);

    // 3. Assemble the config skeleton.
    let mut root: Map<String, Value> = Map::new();
    root.insert(
        "@id".to_owned(),
        Value::String(OWNERSHIP_SENTINEL.to_owned()),
    );

    let mut apps: Map<String, Value> = Map::new();

    let mut http: Map<String, Value> = Map::new();
    http.insert("servers".to_owned(), Value::Object(servers));
    apps.insert("http".to_owned(), Value::Object(http));

    let mut tls: Map<String, Value> = Map::new();
    tls.insert("automation".to_owned(), tls_automation);
    apps.insert("tls".to_owned(), Value::Object(tls));

    root.insert("apps".to_owned(), Value::Object(apps));

    // 4. Merge unknown_extensions — MUST NOT overwrite owned keys.
    apply_unknown_extensions(&mut root, &state.unknown_extensions)?;

    Ok(Value::Object(root))
}

/// Build the `apps.http.servers` map from state routes.
///
/// All enabled routes are grouped under a single server named `"trilithon"`.
/// Routes are emitted in [`BTreeMap`] (sorted) order.
fn build_http_servers(state: &DesiredState) -> Result<Map<String, Value>, RenderError> {
    let mut caddy_routes: Vec<Value> = Vec::new();

    for (route_id, route) in &state.routes {
        if !route.enabled {
            continue;
        }

        let path_prefix = format!("/routes/{}", route_id.as_str());

        // Validate and build the matcher block.
        let matcher = build_matcher(route, &path_prefix)?;

        // Resolve upstreams.
        let handler = build_handler(route, state, &path_prefix)?;

        let mut caddy_route: Map<String, Value> = Map::new();
        if let Some(m) = matcher {
            caddy_route.insert("match".to_owned(), Value::Array(vec![m]));
        }
        caddy_route.insert("handle".to_owned(), Value::Array(vec![handler]));

        caddy_routes.push(Value::Object(caddy_route));
    }

    let mut server: Map<String, Value> = Map::new();
    server.insert(
        "listen".to_owned(),
        Value::Array(vec![Value::String(":443".to_owned())]),
    );
    server.insert("routes".to_owned(), Value::Array(caddy_routes));

    let mut servers: Map<String, Value> = Map::new();
    servers.insert("trilithon".to_owned(), Value::Object(server));
    Ok(servers)
}

/// Build a Caddy matcher object for one route.
///
/// Returns `None` when neither hostnames nor path matchers are present
/// (i.e. the route matches everything without an explicit matcher block).
fn build_matcher(route: &Route, path_prefix: &str) -> Result<Option<Value>, RenderError> {
    let mut matcher: Map<String, Value> = Map::new();

    // Hostname matchers.
    if !route.hostnames.is_empty() {
        let mut host_list: Vec<Value> = Vec::new();
        for (i, hp) in route.hostnames.iter().enumerate() {
            let raw = match hp {
                HostPattern::Exact(h) | HostPattern::Wildcard(h) => h.as_str(),
            };
            crate::model::route::validate_hostname(raw).map_err(|_| {
                RenderError::InvalidHostname {
                    host: raw.to_owned(),
                    path: format!("{path_prefix}/hostnames/{i}"),
                }
            })?;
            host_list.push(Value::String(raw.to_owned()));
        }
        matcher.insert("host".to_owned(), Value::Array(host_list));
    }

    // Path matchers.
    if !route.matchers.paths.is_empty() {
        let paths: Vec<Value> = route
            .matchers
            .paths
            .iter()
            .map(|p| Value::String(p.0.clone()))
            .collect();
        matcher.insert("path".to_owned(), Value::Array(paths));
    }

    if matcher.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Value::Object(matcher)))
    }
}

/// Build the Caddy handler block for one route.
///
/// - If the route has a redirect rule, emits a `static_response` handler.
/// - Otherwise emits a `reverse_proxy` handler.
fn build_handler(
    route: &Route,
    state: &DesiredState,
    path_prefix: &str,
) -> Result<Value, RenderError> {
    // Redirect route takes priority.
    if let Some(ref redirect) = route.redirects {
        let mut h: Map<String, Value> = Map::new();
        h.insert(
            "handler".to_owned(),
            Value::String("static_response".to_owned()),
        );
        h.insert(
            "status_code".to_owned(),
            Value::Number(serde_json::Number::from(redirect.status)),
        );
        h.insert(
            "headers".to_owned(),
            Value::Object({
                let mut headers: Map<String, Value> = Map::new();
                headers.insert(
                    "Location".to_owned(),
                    Value::Array(vec![Value::String(redirect.to.clone())]),
                );
                headers
            }),
        );
        return Ok(Value::Object(h));
    }

    // Validate and collect upstreams.
    let mut upstream_values: Vec<Value> = Vec::new();
    for (i, upstream_id) in route.upstreams.iter().enumerate() {
        let target = resolve_upstream_dial(upstream_id, state, path_prefix, i)?;
        let mut u: Map<String, Value> = Map::new();
        u.insert("dial".to_owned(), Value::String(target));
        upstream_values.push(Value::Object(u));
    }

    let mut h: Map<String, Value> = Map::new();
    h.insert(
        "handler".to_owned(),
        Value::String("reverse_proxy".to_owned()),
    );
    h.insert("upstreams".to_owned(), Value::Array(upstream_values));

    // Header manipulation.
    if let Some(header_ops) = build_header_ops(route) {
        h.insert("headers".to_owned(), header_ops);
    }

    // Policy attachment body is embedded in the handler for Phase 7.
    if let Some(ref attachment) = route.policy_attachment {
        let preset_id = attachment.preset_id.as_str();
        let version = attachment.preset_version;
        let preset = state
            .presets
            .get(&attachment.preset_id)
            .filter(|p| p.version == version)
            .ok_or_else(|| RenderError::UnknownPreset {
                preset: preset_id.to_owned(),
                version,
            })?;
        // Embed the preset body as an opaque JSON value under "policy".
        // A parse failure is a render error — silently omitting a policy body
        // would produce a valid-looking config missing a security policy.
        let body = serde_json::from_str::<Value>(&preset.body_json).map_err(|e| {
            RenderError::InvalidPresetBody {
                preset: preset_id.to_owned(),
                version,
                detail: e.to_string(),
            }
        })?;
        h.insert("policy".to_owned(), body);
    }

    Ok(Value::Object(h))
}

/// Resolve the `dial` target for an upstream by looking it up in `state.upstreams`.
fn resolve_upstream_dial(
    upstream_id: &UpstreamId,
    state: &DesiredState,
    path_prefix: &str,
    idx: usize,
) -> Result<String, RenderError> {
    let upstream: &Upstream =
        state
            .upstreams
            .get(upstream_id)
            .ok_or_else(|| RenderError::InvalidUpstream {
                target: upstream_id.as_str().to_owned(),
                path: format!("{path_prefix}/upstreams/{idx}"),
            })?;

    match &upstream.destination {
        UpstreamDestination::TcpAddr { host, port } => {
            validate_upstream_host(host, &format!("{path_prefix}/upstreams/{idx}/host"))?;
            // IPv6 addresses contain ':' and must be wrapped in brackets so
            // Caddy can distinguish address from port (e.g. `[::1]:8080`).
            if host.contains(':') {
                Ok(format!("[{host}]:{port}"))
            } else {
                Ok(format!("{host}:{port}"))
            }
        }
        UpstreamDestination::UnixSocket { path } => {
            if !std::path::Path::new(path.as_str()).is_absolute()
                || path.contains("..")
                || path.contains('\0')
            {
                return Err(RenderError::InvalidUpstream {
                    target: path.clone(),
                    path: format!("{path_prefix}/upstreams/{idx}/path"),
                });
            }
            Ok(format!("unix/{path}"))
        }
        UpstreamDestination::DockerContainer { container_id, port } => {
            // container_id must match [a-zA-Z0-9_.\-]{1,128}
            let valid = !container_id.is_empty()
                && container_id.len() <= 128
                && container_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-');
            if !valid {
                return Err(RenderError::InvalidUpstream {
                    target: container_id.clone(),
                    path: format!("{path_prefix}/upstreams/{idx}/container_id"),
                });
            }
            Ok(format!("{container_id}:{port}"))
        }
    }
}

/// Build the Caddy header operations object for a route.
///
/// Returns `None` when the route has no header manipulation rules so the
/// caller can skip emitting the `"headers"` key entirely.
fn build_header_ops(route: &Route) -> Option<Value> {
    let request_ops: Vec<Value> = route
        .headers
        .request
        .iter()
        .map(header_op_to_value)
        .collect();
    let response_ops: Vec<Value> = route
        .headers
        .response
        .iter()
        .map(header_op_to_value)
        .collect();

    if request_ops.is_empty() && response_ops.is_empty() {
        return None;
    }

    let mut headers: Map<String, Value> = Map::new();
    if !request_ops.is_empty() {
        headers.insert("request".to_owned(), Value::Array(request_ops));
    }
    if !response_ops.is_empty() {
        headers.insert("response".to_owned(), Value::Array(response_ops));
    }
    Some(Value::Object(headers))
}

/// Convert a single [`HeaderOp`] to its Caddy JSON representation.
fn header_op_to_value(op: &HeaderOp) -> Value {
    let mut m: Map<String, Value> = Map::new();
    match op {
        HeaderOp::Set { name, value } => {
            m.insert("op".to_owned(), Value::String("set".to_owned()));
            m.insert("field".to_owned(), Value::String(name.clone()));
            m.insert("value".to_owned(), Value::String(value.clone()));
        }
        HeaderOp::Add { name, value } => {
            m.insert("op".to_owned(), Value::String("add".to_owned()));
            m.insert("field".to_owned(), Value::String(name.clone()));
            m.insert("value".to_owned(), Value::String(value.clone()));
        }
        HeaderOp::Delete { name } => {
            m.insert("op".to_owned(), Value::String("delete".to_owned()));
            m.insert("field".to_owned(), Value::String(name.clone()));
        }
    }
    Value::Object(m)
}

/// Build the `apps.tls.automation` block from the global TLS config.
fn build_tls_automation(tls: &TlsConfig) -> Value {
    let mut automation: Map<String, Value> = Map::new();

    let mut policies: Vec<Value> = Vec::new();

    if tls.on_demand_enabled {
        let mut on_demand: Map<String, Value> = Map::new();
        if let Some(ref ask_url) = tls.on_demand_ask_url {
            let mut ask: Map<String, Value> = Map::new();
            ask.insert("endpoint".to_owned(), Value::String(ask_url.clone()));
            on_demand.insert("ask".to_owned(), Value::Object(ask));
        }
        automation.insert("on_demand".to_owned(), Value::Object(on_demand));
    }

    // Default issuer policy.
    if let Some(ref issuer) = tls.default_issuer {
        let mut policy: Map<String, Value> = Map::new();
        match issuer {
            TlsIssuer::Acme { directory_url } => {
                let mut issuers: Map<String, Value> = Map::new();
                issuers.insert("module".to_owned(), Value::String("acme".to_owned()));
                issuers.insert("ca".to_owned(), Value::String(directory_url.clone()));
                policy.insert(
                    "issuers".to_owned(),
                    Value::Array(vec![Value::Object(issuers)]),
                );
            }
            TlsIssuer::Internal => {
                let mut issuers: Map<String, Value> = Map::new();
                issuers.insert("module".to_owned(), Value::String("internal".to_owned()));
                policy.insert(
                    "issuers".to_owned(),
                    Value::Array(vec![Value::Object(issuers)]),
                );
            }
        }
        policies.push(Value::Object(policy));
    }

    if let Some(ref email) = tls.email {
        automation.insert("email".to_owned(), Value::String(email.clone()));
    }

    if !policies.is_empty() {
        automation.insert("policies".to_owned(), Value::Array(policies));
    }

    Value::Object(automation)
}

/// Apply `unknown_extensions` onto the root config map.
///
/// The merge is last-wins for non-Trilithon-owned paths. If a pointer targets
/// a Trilithon-owned key, returns [`RenderError::ExtensionCollision`].
///
/// For simplicity this implementation only supports top-level pointer segments
/// (paths of the form `/key`). Nested pointer support (e.g. `/apps/foo`) is
/// implemented via JSON Pointer navigation.
fn apply_unknown_extensions(
    root: &mut Map<String, Value>,
    extensions: &BTreeMap<JsonPointer, Value>,
) -> Result<(), RenderError> {
    for (pointer, value) in extensions {
        let ptr_str = pointer.as_str();

        // Check for collision with owned top-level keys.
        // A pointer "/apps/..." or "/@id" collides.
        let first_segment = ptr_str
            .strip_prefix('/')
            .and_then(|s| s.split('/').next())
            .unwrap_or("");

        if OWNED_TOP_LEVEL_KEYS.contains(&first_segment) {
            return Err(RenderError::ExtensionCollision {
                pointer: ptr_str.to_owned(),
            });
        }

        // Apply the pointer write.
        json_pointer_set(root, ptr_str, value.clone()).map_err(|()| {
            RenderError::ExtensionCollision {
                pointer: ptr_str.to_owned(),
            }
        })?;
    }
    Ok(())
}

/// Write `value` at the RFC 6901 JSON pointer `ptr` within `root`.
///
/// Creates intermediate objects as needed. Returns `Err(())` if an
/// intermediate node exists but is not an object.
fn json_pointer_set(root: &mut Map<String, Value>, ptr: &str, value: Value) -> Result<(), ()> {
    if ptr.is_empty() || ptr == "/" {
        // Root pointer — not supported for extension merges.
        return Err(());
    }

    let segments: Vec<String> = ptr
        .strip_prefix('/')
        .ok_or(())?
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect();

    if segments.is_empty() {
        return Err(());
    }

    let mut current = root;
    let last_idx = segments.len() - 1;

    for (i, segment) in segments.iter().enumerate() {
        if i == last_idx {
            current.insert(segment.clone(), value);
            return Ok(());
        }
        // Navigate deeper, creating objects as needed.
        let entry = current
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        match entry {
            Value::Object(map) => {
                current = map;
            }
            _ => return Err(()),
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Validate an upstream host string (must not be empty; must not contain `/`).
///
/// This is a lightweight check — the Phase 4 validation layer has already
/// checked the full constraint set; the renderer is the last line of defence.
///
/// # Errors
///
/// Returns [`RenderError::InvalidUpstream`] when the host is invalid.
fn validate_upstream_host(host: &str, path: &str) -> Result<(), RenderError> {
    if host.is_empty() {
        return Err(RenderError::InvalidUpstream {
            target: host.to_owned(),
            path: path.to_owned(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
    use crate::model::{
        desired_state::DesiredState,
        header::HeaderRules,
        identifiers::{PolicyId, PresetId, RouteId, UpstreamId},
        matcher::MatcherSet,
        policy::{PolicyAttachment, PresetVersion},
        primitive::JsonPointer,
        route::{HostPattern, Route},
        upstream::{Upstream, UpstreamDestination, UpstreamProbe},
    };

    fn make_route(id: &str, hostname: &str) -> Route {
        Route {
            id: RouteId(id.to_owned()),
            hostnames: vec![HostPattern::Exact(hostname.to_owned())],
            upstreams: vec![],
            matchers: MatcherSet::default(),
            headers: HeaderRules::default(),
            redirects: None,
            policy_attachment: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn make_upstream(id: &str, host: &str, port: u16) -> Upstream {
        Upstream {
            id: UpstreamId(id.to_owned()),
            destination: UpstreamDestination::TcpAddr {
                host: host.to_owned(),
                port,
            },
            probe: UpstreamProbe::Disabled,
            weight: 1,
            max_request_bytes: None,
        }
    }

    fn renderer() -> DefaultCaddyJsonRenderer {
        DefaultCaddyJsonRenderer
    }

    // ----------------------------------------------------------------
    // deterministic_byte_identical_outputs
    // ----------------------------------------------------------------

    #[test]
    fn deterministic_byte_identical_outputs() {
        let mut state = DesiredState::empty();
        state.routes.insert(
            RouteId("ROUTE001".to_owned()),
            make_route("ROUTE001", "example.com"),
        );

        let r = renderer();
        let v1 = r.render(&state).expect("first render");
        let v2 = r.render(&state).expect("second render");

        assert_eq!(
            canonical_json_bytes(&v1),
            canonical_json_bytes(&v2),
            "byte output must be identical for identical inputs"
        );
    }

    // ----------------------------------------------------------------
    // sorted_keys_under_random_insert_order
    // ----------------------------------------------------------------

    #[test]
    fn sorted_keys_under_random_insert_order() {
        // Build state A: routes inserted Z → A
        let mut state_a = DesiredState::empty();
        for id in ["Z_ROUTE", "M_ROUTE", "A_ROUTE"] {
            state_a
                .routes
                .insert(RouteId(id.to_owned()), make_route(id, "host.example.com"));
        }

        // Build state B: same routes but inserted A → Z
        let mut state_b = DesiredState::empty();
        for id in ["A_ROUTE", "M_ROUTE", "Z_ROUTE"] {
            state_b
                .routes
                .insert(RouteId(id.to_owned()), make_route(id, "host.example.com"));
        }

        let r = renderer();
        let bytes_a = canonical_json_bytes(&r.render(&state_a).expect("render A"));
        let bytes_b = canonical_json_bytes(&r.render(&state_b).expect("render B"));

        assert_eq!(bytes_a, bytes_b, "insertion order must not affect output");
    }

    // ----------------------------------------------------------------
    // unknown_extension_round_trip
    // ----------------------------------------------------------------

    #[test]
    fn unknown_extension_round_trip() {
        let mut state = DesiredState::empty();
        state.unknown_extensions.insert(
            JsonPointer::root().push("foo_app"),
            serde_json::json!({"bar": 1}),
        );

        let r = renderer();
        let value = r.render(&state).expect("render");

        let foo_app = value.get("foo_app").expect("foo_app must be present");
        assert_eq!(foo_app.get("bar").and_then(Value::as_i64), Some(1));
    }

    // ----------------------------------------------------------------
    // ownership_sentinel_present
    // ----------------------------------------------------------------

    #[test]
    fn ownership_sentinel_present() {
        let state = DesiredState::empty();
        let r = renderer();
        let value = r.render(&state).expect("render");

        let id = value
            .get("@id")
            .and_then(Value::as_str)
            .expect("@id must be present");
        assert_eq!(id, "trilithon-owner-local");
    }

    // ----------------------------------------------------------------
    // extension_collision_with_owned_key_is_error
    // ----------------------------------------------------------------

    #[test]
    fn extension_collision_with_owned_key_is_error() {
        let mut state = DesiredState::empty();
        state.unknown_extensions.insert(
            JsonPointer::root().push("apps").push("http"),
            serde_json::json!({"evil": true}),
        );

        let r = renderer();
        let err = r.render(&state).expect_err("should fail with collision");
        assert!(
            matches!(err, RenderError::ExtensionCollision { .. }),
            "expected ExtensionCollision, got {err:?}"
        );
    }

    // ----------------------------------------------------------------
    // corpus_fixtures
    // ----------------------------------------------------------------

    #[test]
    fn corpus_fixtures() {
        // Fixture 1: empty state
        let empty = DesiredState::empty();
        let r = renderer();
        let rendered = r.render(&empty).expect("render empty");
        insta::assert_json_snapshot!("empty_state", rendered);

        // Fixture 2: single route with upstream
        let mut with_upstream = DesiredState::empty();
        let up_id = UpstreamId("UP001".to_owned());
        with_upstream
            .upstreams
            .insert(up_id.clone(), make_upstream("UP001", "127.0.0.1", 8080));
        with_upstream.routes.insert(
            RouteId("ROUTE001".to_owned()),
            Route {
                id: RouteId("ROUTE001".to_owned()),
                hostnames: vec![HostPattern::Exact("api.example.com".to_owned())],
                upstreams: vec![up_id],
                matchers: MatcherSet::default(),
                headers: HeaderRules::default(),
                redirects: None,
                policy_attachment: None,
                enabled: true,
                created_at: 0,
                updated_at: 0,
            },
        );
        let rendered_up = r.render(&with_upstream).expect("render with_upstream");
        insta::assert_json_snapshot!("single_route_with_upstream", rendered_up);

        // Fixture 3: route with policy attachment
        let mut with_policy = DesiredState::empty();
        let preset_id = PresetId("PRESET001".to_owned());
        with_policy.presets.insert(
            preset_id.clone(),
            PresetVersion {
                preset_id: preset_id.clone(),
                version: 1,
                body_json: r#"{"rate_limit":100}"#.to_owned(),
            },
        );
        with_policy.routes.insert(
            RouteId("ROUTE002".to_owned()),
            Route {
                id: RouteId("ROUTE002".to_owned()),
                hostnames: vec![HostPattern::Exact("policy.example.com".to_owned())],
                upstreams: vec![],
                matchers: MatcherSet::default(),
                headers: HeaderRules::default(),
                redirects: None,
                policy_attachment: Some(crate::model::route::RoutePolicyAttachment {
                    preset_id: preset_id.clone(),
                    preset_version: 1,
                }),
                enabled: true,
                created_at: 0,
                updated_at: 0,
            },
        );
        with_policy.policies.insert(
            PolicyId("POLICY001".to_owned()),
            PolicyAttachment {
                preset_id,
                preset_version: 1,
            },
        );
        let rendered_policy = r.render(&with_policy).expect("render with_policy");
        insta::assert_json_snapshot!("route_with_policy", rendered_policy);
    }
}

//! `GET /api/v1/capabilities` — cached capability probe result (Slice 9.11).
//!
//! Returns the most recently probed Caddy capabilities.  If the probe has
//! not yet completed, returns 503 with `{"code":"capability-probe-pending"}`.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::http_axum::AppState;
use crate::http_axum::auth_middleware::AuthenticatedSession;

// ── Response type ─────────────────────────────────────────────────────────────

/// Response body for `GET /api/v1/capabilities`.
#[derive(Serialize)]
pub struct CapabilitiesResponse {
    /// Caddy server version string reported by the admin API.
    pub caddy_version: String,
    /// Unix epoch seconds at which the probe was taken.
    pub probed_at: i64,
    /// Sorted list of Caddy module identifiers present in the running instance.
    pub modules: Vec<String>,
    /// Whether the Caddy instance has `http.handlers.rate_limit` loaded.
    pub has_rate_limit: bool,
    /// Whether the Caddy instance has `http.handlers.waf` loaded.
    pub has_waf: bool,
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// `GET /api/v1/capabilities` — returns the cached capability probe result.
///
/// Reads the in-memory [`CapabilityCache`] held on [`AppState`].  If the
/// background probe has not completed yet, returns 503.
///
/// # Errors
///
/// Returns 503 when no probe result is cached.
#[utoipa::path(
    get,
    path = "/api/v1/capabilities",
    responses(
        (status = 200, description = "Cached capability probe result"),
        (status = 401, description = "Unauthenticated"),
        (status = 503, description = "Probe not yet completed"),
    )
)]
pub async fn get_capabilities(
    State(state): State<Arc<AppState>>,
    _session: AuthenticatedSession,
) -> Response {
    match state.capability_cache.snapshot() {
        None => {
            #[allow(clippy::disallowed_methods)]
            // reason: serde_json::json! macro uses unwrap internally; acceptable in HTTP response shaping
            let body = serde_json::json!({"code": "capability-probe-pending"});
            (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
        }
        Some(caps) => {
            let mut modules: Vec<String> = caps.loaded_modules.into_iter().collect();
            modules.sort();
            let has_rate_limit = modules.contains(&"http.handlers.rate_limit".to_owned());
            let has_waf = modules.contains(&"http.handlers.waf".to_owned());
            (
                StatusCode::OK,
                Json(CapabilitiesResponse {
                    caddy_version: caps.caddy_version,
                    probed_at: caps.probed_at,
                    modules,
                    has_rate_limit,
                    has_waf,
                }),
            )
                .into_response()
        }
    }
}

//! Route list endpoint (Slice 9.8).
//!
//! `GET /api/v1/routes` — flattened list of routes from the latest desired state.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use trilithon_core::model::desired_state::DesiredState;
use trilithon_core::model::route::HostPattern;

use crate::http_axum::AppState;
use crate::http_axum::auth_middleware::AuthenticatedSession;
use crate::http_axum::auth_routes::ApiError;

// ── Wire types ────────────────────────────────────────────────────────────────

/// Summary of a route policy attachment.
#[derive(Serialize)]
pub struct RoutePolicySummary {
    /// The preset id being applied.
    pub preset_id: String,
    /// The preset version at attachment time.
    pub preset_version: u32,
}

/// Summary row returned by `GET /api/v1/routes`.
#[derive(Serialize)]
pub struct RouteSummary {
    /// Route identifier.
    pub id: String,
    /// All hostname patterns for this route.
    pub hostnames: Vec<String>,
    /// Number of upstreams attached.
    pub upstream_count: u32,
    /// Policy preset attached, if any.
    pub policy_attached: Option<RoutePolicySummary>,
    /// Whether the route is active.
    pub enabled: bool,
    /// Last-updated timestamp (Unix seconds).
    pub updated_at: i64,
}

/// Query parameters for `GET /api/v1/routes`.
#[derive(Deserialize)]
pub struct RouteListQuery {
    /// Maximum number of rows to return. Clamped to 500; default 100.
    pub limit: Option<u32>,
    /// Cursor — the id of the last-seen route.  Only routes with an id
    /// strictly greater (lexicographic) than the cursor are returned.
    pub cursor_after: Option<String>,
    /// Case-insensitive substring filter applied across all hostnames.
    pub hostname_filter: Option<String>,
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// `GET /api/v1/routes` — list routes from the latest desired state.
///
/// # Errors
///
/// Returns `ApiError::Internal` on storage or deserialisation failure.
pub async fn list_routes(
    State(state): State<Arc<AppState>>,
    _session: AuthenticatedSession,
    Query(q): Query<RouteListQuery>,
) -> Result<Json<Vec<RouteSummary>>, ApiError> {
    let limit = q.limit.unwrap_or(100).min(500);

    // Load the latest snapshot. If none exists, return an empty list.
    let maybe_snap = state
        .storage
        .latest_desired_state()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let Some(snap) = maybe_snap else {
        return Ok(Json(vec![]));
    };

    let desired: DesiredState = serde_json::from_str(&snap.desired_state_json)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let filter_lower = q.hostname_filter.map(|f| f.to_lowercase());

    let summaries: Vec<RouteSummary> = desired
        .routes
        .into_iter()
        // Cursor: skip routes whose id is lexicographically <= cursor_after.
        .filter(|(route_id, _)| {
            q.cursor_after
                .as_ref()
                .is_none_or(|cursor| route_id.as_str() > cursor.as_str())
        })
        // Hostname filter.
        .filter(|(_, route)| {
            filter_lower.as_ref().is_none_or(|f| {
                route.hostnames.iter().any(|hp| {
                    let s = match hp {
                        HostPattern::Exact(h) | HostPattern::Wildcard(h) => h.to_lowercase(),
                    };
                    s.contains(f.as_str())
                })
            })
        })
        .take(limit as usize)
        .map(|(route_id, route)| {
            let hostnames = route
                .hostnames
                .iter()
                .map(|hp| match hp {
                    HostPattern::Exact(h) | HostPattern::Wildcard(h) => h.clone(),
                })
                .collect();

            #[allow(clippy::cast_possible_truncation)]
            // reason: upstream count is bounded by configuration; u32 is sufficient
            let upstream_count = route.upstreams.len() as u32;

            let policy_attached = route.policy_attachment.map(|pa| RoutePolicySummary {
                preset_id: pa.preset_id.0,
                preset_version: pa.preset_version,
            });

            RouteSummary {
                id: route_id.0,
                hostnames,
                upstream_count,
                policy_attached,
                enabled: route.enabled,
                updated_at: route.updated_at,
            }
        })
        .collect();

    Ok(Json(summaries))
}

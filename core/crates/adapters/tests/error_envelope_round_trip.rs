//! For each `ApiError` variant, verify `into_response()` returns the right
//! status code and JSON `code` field.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: unit-style test — panics are the correct failure mode

use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use trilithon_adapters::http_axum::auth_routes::ApiError;

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let (_, body) = resp.into_parts();
    let bytes = to_bytes(body, 64 * 1024).await.expect("read body");
    serde_json::from_slice(&bytes).expect("parse JSON")
}

#[tokio::test]
async fn unauthenticated_is_401() {
    let resp = ApiError::Unauthenticated.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let json = body_json(resp).await;
    assert_eq!(json["code"], "unauthenticated");
}

#[tokio::test]
async fn not_found_is_404() {
    let resp = ApiError::NotFound.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let json = body_json(resp).await;
    assert_eq!(json["code"], "not-found");
}

#[tokio::test]
async fn forbidden_is_403() {
    let resp = ApiError::Forbidden {
        code: "must-change-password",
    }
    .into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let json = body_json(resp).await;
    assert_eq!(json["code"], "must-change-password");
}

#[tokio::test]
async fn rate_limited_is_429_with_retry_after() {
    let resp = ApiError::RateLimited {
        retry_after_seconds: 30,
    }
    .into_response();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = resp
        .headers()
        .get("Retry-After")
        .expect("Retry-After header")
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(retry_after, "30");
}

#[tokio::test]
async fn internal_is_500() {
    let resp = ApiError::Internal("db exploded".to_owned()).into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

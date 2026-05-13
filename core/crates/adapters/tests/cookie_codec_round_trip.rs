//! `parse_cookie(build_cookie(...))` round-trips to the original session id.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
// reason: test-only

use http::HeaderMap;
use trilithon_adapters::auth::{build_cookie, parse_cookie};

#[test]
fn cookie_codec_round_trip_without_secure() {
    let session_id = "abc123xyz";
    let cookie_header = build_cookie("sid", session_id, 3600, false);

    let mut map = HeaderMap::new();
    // build_cookie produces a Set-Cookie value. Simulate the browser sending it
    // back as a Cookie header.
    let cookie_value = cookie_header.to_str().unwrap();
    // Extract just the name=value portion (before the first ';').
    let name_value = cookie_value.split(';').next().unwrap().trim();
    map.insert(
        http::header::COOKIE,
        http::HeaderValue::from_str(name_value).unwrap(),
    );

    let parsed = parse_cookie(&map, "sid");
    assert_eq!(parsed.as_deref(), Some(session_id));
}

#[test]
fn cookie_codec_round_trip_with_secure() {
    let session_id = "secure-token-42";
    let cookie_header = build_cookie("auth", session_id, 7200, true);
    let value_str = cookie_header.to_str().unwrap();
    assert!(
        value_str.contains("; Secure"),
        "Secure flag must be present"
    );

    let name_value = value_str.split(';').next().unwrap().trim();
    let mut map = HeaderMap::new();
    map.insert(
        http::header::COOKIE,
        http::HeaderValue::from_str(name_value).unwrap(),
    );

    let parsed = parse_cookie(&map, "auth");
    assert_eq!(parsed.as_deref(), Some(session_id));
}

#[test]
fn cookie_codec_missing_name_returns_none() {
    let mut map = HeaderMap::new();
    map.insert(
        http::header::COOKIE,
        http::HeaderValue::from_static("other=value"),
    );
    assert!(parse_cookie(&map, "sid").is_none());
}

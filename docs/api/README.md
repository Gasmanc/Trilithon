# Trilithon Daemon API

The Trilithon daemon exposes a REST API over HTTP (default: `http://127.0.0.1:7878`).

## OpenAPI Document

The full OpenAPI 3.1 document is available at:

```
GET /api/v1/openapi.json
```

No authentication is required to fetch the OpenAPI document.

## Authentication

All endpoints except `/api/v1/health` and `/api/v1/openapi.json` require authentication.

Two authentication methods are supported:

### Session cookie

1. Call `POST /api/v1/auth/login` with `{"username": "...", "password": "..."}`.
2. The response sets a `trilithon_session` cookie.
3. Include that cookie in subsequent requests.

Session cookies are stored server-side and can be revoked via `POST /api/v1/auth/logout`.

### Bearer token

Include a pre-generated API token as a Bearer token in the `Authorization` header:

```
Authorization: Bearer <token>
```

Token authentication must be enabled at daemon startup (requires a SQLite pool with the token table).

## Default binding

The daemon binds to `127.0.0.1:7878` by default (loopback only).  Remote binding requires `network.allow_remote_binding = true` in the daemon configuration.

## Bootstrap flow

On first start, the daemon detects that no users exist and:

1. Generates a random password for the `admin` account.
2. Writes credentials to `<data-dir>/bootstrap-credentials.txt` with file mode `0600`.
3. Sets `must_change_pw = true` on the account.

The initial login returns `409 Conflict` with `{"code":"must-change-password"}` and issues a session cookie.  The client must call `POST /api/v1/auth/change-password` before any other endpoint is accessible.

## Error envelope

All error responses use a JSON body with a `code` field:

```json
{ "code": "<machine-readable-code>", "detail": "<optional human-readable detail>" }
```

| HTTP status | code |
|-------------|------|
| 401 | `unauthenticated` |
| 403 | `must-change-password` (or other forbidden codes) |
| 404 | `not-found` |
| 409 | `conflict` |
| 422 | `schema-error` |
| 429 | `rate-limited` (also sets `Retry-After` header) |
| 503 | `capability-probe-pending` or `lock-contested` |
| 500 | `internal` |

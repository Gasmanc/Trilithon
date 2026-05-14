-- Slice 9.6: API bearer tokens table.
-- Each row represents a long-lived token that can authenticate API requests.
-- The raw token is never stored; only its SHA-256 hex digest is persisted.
CREATE TABLE IF NOT EXISTS tokens (
    token_id    TEXT NOT NULL PRIMARY KEY,   -- ULID or UUID assigned at creation
    token_hash  TEXT NOT NULL UNIQUE,        -- lowercase hex of SHA-256(raw_bearer_token)
    permissions TEXT NOT NULL DEFAULT '{}',  -- JSON blob of permission flags
    rate_limit_qps INTEGER NOT NULL DEFAULT 10,
    created_at  INTEGER NOT NULL,            -- Unix seconds
    revoked_at  INTEGER                      -- NULL = active
);

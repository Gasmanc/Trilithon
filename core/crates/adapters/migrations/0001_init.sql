-- core/crates/adapters/migrations/0001_init.sql
PRAGMA foreign_keys = ON;

CREATE TABLE schema_migrations (
    version       INTEGER PRIMARY KEY,
    applied_at    INTEGER NOT NULL,
    description   TEXT NOT NULL,
    checksum      TEXT NOT NULL
);

CREATE TABLE caddy_instances (
    id              TEXT PRIMARY KEY,
    display_name    TEXT NOT NULL,
    transport       TEXT NOT NULL CHECK (transport IN ('unix', 'loopback_mtls')),
    address         TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    last_seen_at    INTEGER,
    capability_json TEXT,
    ownership_token TEXT NOT NULL
);

CREATE TABLE users (
    id               TEXT PRIMARY KEY,
    caddy_instance_id TEXT NOT NULL DEFAULT 'local',
    username         TEXT NOT NULL UNIQUE,
    password_hash    TEXT NOT NULL,
    role             TEXT NOT NULL CHECK (role IN ('owner', 'operator', 'reader')),
    created_at       INTEGER NOT NULL,
    must_change_pw   INTEGER NOT NULL DEFAULT 0,
    disabled_at      INTEGER
);
CREATE INDEX users_disabled_at ON users(disabled_at);

CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,
    caddy_instance_id TEXT NOT NULL DEFAULT 'local',
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL,
    expires_at      INTEGER NOT NULL,
    revoked_at      INTEGER,
    user_agent      TEXT,
    ip_address      TEXT
);
CREATE INDEX sessions_user_id ON sessions(user_id);
CREATE INDEX sessions_expires_at ON sessions(expires_at);

CREATE TABLE snapshots (
    id                  TEXT PRIMARY KEY,
    parent_id           TEXT REFERENCES snapshots(id),
    caddy_instance_id   TEXT NOT NULL DEFAULT 'local',
    actor_kind          TEXT NOT NULL CHECK (actor_kind IN ('user', 'token', 'system')),
    actor_id            TEXT NOT NULL,
    intent              TEXT NOT NULL,
    correlation_id      TEXT NOT NULL,
    caddy_version       TEXT NOT NULL,
    trilithon_version   TEXT NOT NULL,
    created_at          INTEGER NOT NULL,
    created_at_ms       INTEGER NOT NULL,
    config_version      INTEGER NOT NULL,
    desired_state_json  TEXT NOT NULL
);
CREATE INDEX snapshots_parent_id ON snapshots(parent_id);
CREATE INDEX snapshots_correlation_id ON snapshots(correlation_id);
CREATE INDEX snapshots_caddy_instance_id ON snapshots(caddy_instance_id);
CREATE UNIQUE INDEX snapshots_config_version ON snapshots(caddy_instance_id, config_version);

CREATE TABLE audit_log (
    id                 TEXT PRIMARY KEY,
    caddy_instance_id  TEXT NOT NULL DEFAULT 'local',
    correlation_id     TEXT NOT NULL,
    occurred_at        INTEGER NOT NULL,
    occurred_at_ms     INTEGER NOT NULL,
    actor_kind         TEXT NOT NULL,
    actor_id           TEXT NOT NULL,
    kind               TEXT NOT NULL,
    target_kind        TEXT,
    target_id          TEXT,
    snapshot_id        TEXT REFERENCES snapshots(id),
    redacted_diff_json TEXT,
    redaction_sites    INTEGER NOT NULL DEFAULT 0,
    outcome            TEXT NOT NULL CHECK (outcome IN ('ok', 'error', 'denied')),
    error_kind         TEXT,
    notes              TEXT
);
CREATE INDEX audit_log_correlation_id ON audit_log(correlation_id);
CREATE INDEX audit_log_occurred_at ON audit_log(occurred_at);
CREATE INDEX audit_log_actor_id ON audit_log(actor_id);
CREATE INDEX audit_log_kind ON audit_log(kind);

CREATE TABLE mutations (
    id                  TEXT PRIMARY KEY,
    caddy_instance_id   TEXT NOT NULL DEFAULT 'local',
    correlation_id      TEXT NOT NULL,
    submitted_by_kind   TEXT NOT NULL,
    submitted_by_id     TEXT NOT NULL,
    submitted_at        INTEGER NOT NULL,
    expected_version    INTEGER NOT NULL,
    payload_json        TEXT NOT NULL,
    state               TEXT NOT NULL CHECK (state IN ('queued', 'validating', 'applying', 'applied', 'rejected', 'failed')),
    state_changed_at    INTEGER NOT NULL,
    result_snapshot_id  TEXT REFERENCES snapshots(id),
    failure_kind        TEXT,
    failure_message     TEXT
);
CREATE INDEX mutations_state ON mutations(state);
CREATE INDEX mutations_correlation_id ON mutations(correlation_id);

CREATE TABLE secrets_metadata (
    id                TEXT PRIMARY KEY,
    caddy_instance_id TEXT NOT NULL DEFAULT 'local',
    owner_kind        TEXT NOT NULL,
    owner_id          TEXT NOT NULL,
    field_path        TEXT NOT NULL,
    nonce             BLOB NOT NULL,
    ciphertext        BLOB NOT NULL,
    created_at        INTEGER NOT NULL,
    rotated_at        INTEGER,
    last_revealed_at  INTEGER,
    last_revealed_by  TEXT
);

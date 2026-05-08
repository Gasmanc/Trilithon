-- core/crates/adapters/migrations/0006_audit_immutable.sql
--
-- Enforce audit_log immutability at the database layer (ADR-0009 / architecture §6.6).
-- The audit_log table was created by migration 0001; this migration adds the immutability
-- triggers so that any UPDATE or DELETE on audit_log rows is rejected with a database-level
-- ABORT.  CREATE TABLE IF NOT EXISTS is included for safety in environments where 0001 was
-- applied without the audit_log table (should never happen in practice).

CREATE TABLE IF NOT EXISTS audit_log (
    id                 TEXT PRIMARY KEY,
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
CREATE INDEX IF NOT EXISTS audit_log_correlation_id ON audit_log(correlation_id);
CREATE INDEX IF NOT EXISTS audit_log_occurred_at    ON audit_log(occurred_at);
CREATE INDEX IF NOT EXISTS audit_log_actor_id       ON audit_log(actor_id);
CREATE INDEX IF NOT EXISTS audit_log_kind           ON audit_log(kind);

-- Immutability triggers.
CREATE TRIGGER audit_log_no_update
BEFORE UPDATE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log rows are immutable (architecture §6.6)');
END;

CREATE TRIGGER audit_log_no_delete
BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log rows are immutable (architecture §6.6)');
END;

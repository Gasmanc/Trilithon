-- Drift event tracking table (Slice 8.6).
CREATE TABLE IF NOT EXISTS drift_events (
    id                 TEXT PRIMARY KEY NOT NULL,
    correlation_id     TEXT NOT NULL,
    detected_at        INTEGER NOT NULL,
    snapshot_id        TEXT NOT NULL,
    diff_json          TEXT NOT NULL,
    running_state_hash TEXT NOT NULL,
    resolution         TEXT,
    resolved_at        INTEGER
);

CREATE INDEX IF NOT EXISTS idx_drift_events_unresolved
    ON drift_events (resolved_at)
    WHERE resolved_at IS NULL;

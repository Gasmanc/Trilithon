-- Add UNIQUE constraint on drift_events.correlation_id (F015).
--
-- Without this constraint, resolve_drift_event UPDATE by correlation_id could
-- silently update multiple rows if duplicates exist.  The UNIQUE index both
-- prevents duplicates and makes the rows_affected() == 1 check meaningful.
CREATE UNIQUE INDEX IF NOT EXISTS idx_drift_events_correlation_id_unique
    ON drift_events (correlation_id);

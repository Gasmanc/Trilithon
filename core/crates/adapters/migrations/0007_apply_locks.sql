-- core/crates/adapters/migrations/0007_apply_locks.sql
--
-- Per-instance advisory lock table (Slice 7.6 / architecture §9 / ADR-0012).
-- Enforces at-most-one apply-in-flight per caddy_instance_id across multiple
-- daemon processes sharing the same database file.

CREATE TABLE IF NOT EXISTS apply_locks (
    instance_id TEXT PRIMARY KEY,
    holder_pid  INTEGER NOT NULL,
    acquired_at INTEGER NOT NULL   -- unix timestamp (seconds)
);

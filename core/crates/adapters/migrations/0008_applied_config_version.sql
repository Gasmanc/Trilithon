-- core/crates/adapters/migrations/0008_applied_config_version.sql
--
-- Adds an explicit `applied_config_version` column to `caddy_instances` so
-- that `current_config_version()` reads the last *applied* version, not
-- MAX(snapshots.config_version).  The latter is always >= the applied version
-- because the mutation pipeline inserts the new snapshot before `apply()` is
-- called (Slice 7.5 / ADR-0012).

ALTER TABLE caddy_instances
    ADD COLUMN applied_config_version INTEGER NOT NULL DEFAULT 0;

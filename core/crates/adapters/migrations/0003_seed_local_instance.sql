-- core/crates/adapters/migrations/0003_seed_local_instance.sql
--
-- Seed the default "local" caddy_instances row so that fresh installations
-- satisfy the FK constraint in capability_probe_results before any runtime
-- code inserts a caddy_instances row.  The 'local' id is the compile-time
-- constant CADDY_INSTANCE_ID used by run.rs (Phase 5 will replace this with
-- a database-backed identifier).
INSERT OR IGNORE INTO caddy_instances
    (id, display_name, transport, address, created_at, ownership_token)
VALUES
    ('local', 'Local', 'unix', '', 0, '');

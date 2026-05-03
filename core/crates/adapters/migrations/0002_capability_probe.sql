-- core/crates/adapters/migrations/0002_capability_probe.sql
CREATE TABLE capability_probe_results (
    id                  TEXT PRIMARY KEY,
    caddy_instance_id   TEXT NOT NULL REFERENCES caddy_instances(id),
    probed_at           INTEGER NOT NULL,
    caddy_version       TEXT NOT NULL,
    capabilities_json   TEXT NOT NULL,
    is_current          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX capability_probe_results_instance ON capability_probe_results(caddy_instance_id, probed_at);
CREATE UNIQUE INDEX capability_probe_results_current
    ON capability_probe_results(caddy_instance_id) WHERE is_current = 1;

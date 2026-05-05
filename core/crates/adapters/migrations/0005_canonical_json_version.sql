-- core/crates/adapters/migrations/0005_canonical_json_version.sql
--
-- Add canonical_json_version column to snapshots so that future format changes
-- can be detected without re-hashing all historical data (ADR-0009).
-- Existing rows receive the default value of 1 (the current format version).

ALTER TABLE snapshots ADD COLUMN canonical_json_version INTEGER NOT NULL DEFAULT 1;

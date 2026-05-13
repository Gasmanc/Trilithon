-- core/crates/adapters/migrations/0006_audit_immutable.sql
--
-- Enforce audit_log immutability at the database layer (ADR-0009 / architecture §6.6).
-- The audit_log table is created by migration 0001 (with prev_hash and
-- caddy_instance_id columns); this migration only installs the immutability
-- triggers.  No fallback CREATE TABLE is included: migrations must run in
-- order, and a divergent fallback schema (missing prev_hash / caddy_instance_id)
-- would cause INSERTs to fail at runtime.  If the table is missing, the
-- triggers below will fail to create — which is the correct, loud behaviour.

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

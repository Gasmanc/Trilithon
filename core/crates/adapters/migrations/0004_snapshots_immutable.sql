-- core/crates/adapters/migrations/0004_snapshots_immutable.sql
--
-- Enforce snapshot immutability at the database layer (ADR-0009).
-- Once a row is written to `snapshots`, no UPDATE or DELETE is permitted.
-- Any attempt raises ABORT, which rolls back the enclosing statement/transaction.

CREATE TRIGGER snapshots_no_update
    BEFORE UPDATE ON snapshots
BEGIN
    SELECT RAISE(ABORT, 'snapshots are immutable: UPDATE is forbidden');
END;

CREATE TRIGGER snapshots_no_delete
    BEFORE DELETE ON snapshots
BEGIN
    SELECT RAISE(ABORT, 'snapshots are immutable: DELETE is forbidden');
END;

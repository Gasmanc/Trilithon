| Unit | Title | Type | Date | Commit |
|------|-------|------|------|--------|
| 7.7 | Removed unnecessary `status as u16` cast — CaddyError::BadStatus.status is already u16 — core/crates/adapters/src/applier_caddy.rs | quality | 2026-05-09 | 9e2f66a |
| 7.7 | Moved NeverCalledClient to module scope to fix clippy::items_after_statements — core/crates/adapters/tests/apply_exactly_one_terminal_row.rs | quality | 2026-05-09 | 9e2f66a |
| 7.6 | Replaced `let _ = spawn_blocking` with `drop(spawn_blocking(...))` to fix clippy::let_underscore_future — core/crates/adapters/src/storage_sqlite/locks.rs | quality | 2026-05-09 | 868e852 |
| 7.6 | Changed process_alive to `.is_ok_and()` per clippy::map_unwrap_or — core/crates/adapters/src/storage_sqlite/locks.rs | quality | 2026-05-09 | 868e852 |
| 7.6 | Replaced over-indented doc list items with prose to satisfy clippy::doc_overindented_list_items — core/crates/adapters/src/storage_sqlite/locks.rs | quality | 2026-05-09 | 868e852 |
| 7.6 | Removed unused AtomicI32 import from 32-caller test — core/crates/adapters/tests/apply_serial_under_32_concurrent_callers.rs | quality | 2026-05-09 | 868e852 |
| 7.5 | Added StorageError import; simplified OptimisticConflict match arm from full path — core/crates/adapters/src/applier_caddy.rs | quality | 2026-05-09 | a4921b7 |
| 7.5 | Removed spurious instance_id filter using actor field as proxy; iterate all snapshots — core/crates/core/src/storage/in_memory.rs | quality | 2026-05-09 | a4921b7 |
| 7.3 | Removed unused BTreeSet import — core/crates/core/src/reconciler/capability_check.rs | quality | 2026-05-09 | 12c62ae |
| 7.2 | Removed stale ApplyOutcome::from_error forward-reference from ApplyError doc comment — core/crates/core/src/reconciler/applier.rs | quality | 2026-05-09 | e02be9b |
| 7.1 | Removed duplicate canonicalise function — core/crates/core/src/reconciler/render.rs | reuse | 2026-05-09 | a8a780c |
| 7.1 | Inlined validate_hostname_for_render thin wrapper — core/crates/core/src/reconciler/render.rs | quality | 2026-05-09 | a8a780c |

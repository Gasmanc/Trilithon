| Unit | Title | Type | Date | Commit |
|------|-------|------|------|--------|
| phase-end simplify | Unified unreachable audit branches — core/crates/adapters/src/applier_caddy.rs | Simplify | 2026-05-10 | 9f22604 |
| phase-end simplify | Safer conflict audit notes format — core/crates/adapters/src/applier_caddy.rs | Simplify | 2026-05-10 | 9f22604 |
| phase-end simplify | build_header_ops returns Option<Value> — core/crates/core/src/reconciler/render.rs | Simplify | 2026-05-10 | 9f22604 |
| phase-end simplify | Fix stale module-doc comment — core/crates/adapters/src/applier_caddy.rs | Simplify | 2026-05-10 | 9f22604 |
| phase-end simplify | Two timeout paths in tls_observer — correct, skip — core/crates/adapters/src/tls_observer.rs | Simplify-skip | 2026-05-10 | — |
| phase-end simplify | InMemory CAS stricter than SQLite — intentional, skip — core/crates/core/src/storage/in_memory.rs | Simplify-skip | 2026-05-10 | — |
| multi-review | TRY_INSERT_LOCK_USES_DEFERRED_NOT_IMMEDIATE — core/crates/adapters/src/storage_sqlite/locks.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | COMMIT FAILURE SILENTLY RETURNS Ok — core/crates/adapters/src/sqlite_storage.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | TLS OBSERVER IS DEAD CODE: EMPTY HOSTNAMES ALWAYS PASSED — core/crates/adapters/src/applier_caddy.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | InMemoryStorage CAS DIVERGES FROM SQLITE — core/crates/core/src/storage/in_memory.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | CAS advance UPDATE does not verify row existence — core/crates/adapters/src/storage_sqlite/snapshots.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | CADDY_5XX_PATH_IS_MISCLASSIFIED_AND_UNAUDITED — core/crates/adapters/src/applier_caddy.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | IPv6 upstream addresses rendered without brackets — core/crates/core/src/reconciler/render.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | STALE LOCK REAP RACE: AlreadyHeld REPORTS OWN PID — core/crates/adapters/src/storage_sqlite/locks.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | SHELL SUBPROCESS FOR PROCESS LIVENESS CHECK — core/crates/adapters/src/storage_sqlite/locks.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | notes_to_string AND sort_keys DUPLICATED — core/crates/adapters/src/applier_caddy.rs + tls_observer.rs | Multi-review | 2026-05-10 | a795a7c |
| multi-review | CONFLICT_OUTCOME_VERSIONS_SWAPPED — core/crates/adapters/src/applier_caddy.rs | Multi-review-skip | 2026-05-10 | — |
| multi-review | PHANTOM APPLIED VERSION ON PANIC OR 5XX AFTER CAS — core/crates/adapters/src/applier_caddy.rs | Multi-review-skip | 2026-05-10 | — |
| multi-review | CONFLICT AUDIT NOTE USES HAND-ROLLED JSON — core/crates/adapters/src/applier_caddy.rs | Multi-review-skip | 2026-05-10 | — |
| 7.8 | Changed tls_observer.as_ref().cloned() to .clone() to fix clippy::option_as_ref_cloned — core/crates/adapters/src/applier_caddy.rs | quality | 2026-05-10 | d45dcbb |
| 7.8 | Changed is_none_or closure to method reference to fix clippy::redundant_closure_for_method_calls — core/crates/adapters/tests/apply_emits_tls_issuance_followup_row.rs | quality | 2026-05-10 | d45dcbb |
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

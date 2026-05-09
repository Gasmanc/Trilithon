<!-- No unfixed items for Slice 7.7 -->
<!-- No unfixed items for Slice 7.1 -->
<!-- No unfixed items for Slice 7.4 -->
<!-- No unfixed items for Slice 7.5 -->

## Slice 7.6
- **Pre-existing lib-test failures** — `storage_sqlite::snapshots::tests::advance_succeeds_when_versions_match` and `advance_returns_conflict_when_versions_mismatch` were already failing on `main` before Slice 7.6 was implemented. These tests exercise `advance_config_version_if_eq` with a `BEGIN IMMEDIATE` nested inside an already-begun transaction; the fix would require restructuring those unit tests or the snapshot helper. Deferred to a future cleanup slice.

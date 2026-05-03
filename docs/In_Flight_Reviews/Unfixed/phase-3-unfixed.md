## slice-3.5 — duplicated sqlx_err helper
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** `sqlx_err` in `capability_store.rs` is an exact copy of the same function in `sqlite_storage.rs`. The "three uses before extracting" rule in CLAUDE.md means extraction to a shared private module is not warranted until a third consumer appears.

## slice-3.4 — collect_module_ids unbounded recursion
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** `collect_module_ids` recurses into every node of the Caddy JSON config with no depth limit. A pathological or adversarially crafted config could overflow the stack. The production risk is low because Caddy configs are operator-supplied and bounded in practice, and adding a depth counter would require a breaking signature change or a wrapper. Left for a dedicated hardening pass.

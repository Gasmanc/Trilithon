# Phase 02 — SQLite Persistence — Review Log

## Slice 2.2
**Status:** complete
**Summary:** Created `InMemoryStorage` as a `#![cfg(test)]`-gated struct in `core/crates/core/src/storage/in_memory.rs` implementing all `Storage` trait methods using `std::sync::Mutex`-backed collections. Added the §6.6 audit kind vocabulary in `audit_vocab.rs` (not cfg-gated, so adapters can import it in future slices). All seven contract tests pass.

### Simplify Findings
- `tail_audit_log` chains `.rev()` directly onto the filter iterator, avoiding an intermediate `Vec` allocation.
- `dequeue_proposal` collapses to `pos.map(|idx| proposals.remove(idx))` — a single-expression return.
- `Default` impl delegates to `new()` per Rust convention; `is_none_or` used instead of `map_or(true, …)` as suggested by Clippy.
- No redundant code found; vocabulary lives exactly once in `audit_vocab.rs`.

### Fixes Applied
- Clippy: added `significant_drop_tightening` to module-level `#[allow]` (mutex guards are intentionally broad in a test double).
- Clippy: replaced `map_or(true, …)` with `is_none_or(…)` per `unnecessary_map_or` lint.
- Clippy: removed unused `ProposalSource` and `DriftEventRow`/`DriftRowId` imports from the test module.
- Clippy: added `Default` impl for `InMemoryStorage`.
- Formatter: applied `cargo fmt` formatting pass (two minor whitespace diffs).

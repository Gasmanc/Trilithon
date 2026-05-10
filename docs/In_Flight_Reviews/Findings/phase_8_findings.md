## Slice 8.1 — DiffEngine structural diff over canonical JSON
**Date:** 2026-05-10
**Files reviewed:**
- core/crates/core/src/diff.rs
- core/crates/core/src/diff/flatten.rs

### Findings

**F1** — `diff.rs` `state_to_value`, lines ~25–30 — EFFICIENCY — Original implementation serialized to `Vec<u8>` via `to_canonical_bytes` then deserialized back with `serde_json::from_slice`. Two full serialization passes for no reason. — Replace with `serde_json::to_value(state).map(canonicalise_value)`.

**F2** — `diff.rs` `structural_diff`, ~60 lines — EFFICIENCY — Original used three passes over BTreeMap keys: collect before-keys into a BTreeSet, collect after-keys into a BTreeSet, then iterate both for Added/Removed/Modified classification. BTreeMap iteration is already sorted; one pass over before-map + one pass over after-map suffices. — Replaced with two-pass single-scan approach.

**F3** — `diff.rs` `is_ignored` / `kind_name` / `DiffEntry::path` — QUALITY — Three non-mutating accessor functions were not declared `const` despite being trivially const-eligible; clippy would flag this. — Added `const` to all three.

**F4** — `diff.rs` `apply_diff`, leaf-removal logic — CORRECTNESS — Leaf-by-leaf removal of `/upstreams/U1/...` left empty `{}` containers that broke `serde_json::from_value` deserialization (missing `kind` field on `#[serde(tag = "kind")]` enum). — Introduced `fully_removed_subtrees(flat_before, removed_leaves)`: detects when all leaves of a subtree are being removed and applies removal at the subtree root level, avoiding empty-container residue.

**F5** — `diff.rs` module doc, stale SAFETY comment — QUALITY — A SAFETY comment referred to cycle detection logic that was removed; the comment was misleading. — Removed the stale SAFETY comment.

**F6** — `diff.rs` `flatten_into`, redundant `is_index` binding — QUALITY — Redundant `let is_index = ...` variable computed but not reused; leftover from an earlier design. — Inlined the expression.

**F7** — Gate failure: `caddy_sentinel_e2e` compile error — PRE-EXISTING — `trilithon-adapters` test `caddy_sentinel_e2e` fails to compile in `cargo test --workspace --all-features` with `can't find crate for trilithon_core` and `can't find crate for tokio`. This failure exists at HEAD before Slice 8.1 changes (test added in Phase 3, commit e55bb18). The test lacks a `required-features` guard in Cargo.toml. Not introduced by this slice.

### Items Fixed Inline
- F1: replaced two-pass serialization with `serde_json::to_value(state).map(canonicalise_value)` (commit 057d448)
- F2: replaced three-pass BTreeSet diff with single two-pass scan (commit 057d448)
- F3: added `const` to `is_ignored`, `kind_name`, `DiffEntry::path` (commit 057d448)
- F4: introduced `fully_removed_subtrees` for subtree-level removal in `apply_diff` (commit 057d448)
- F5: removed stale SAFETY comment (commit 057d448)
- F6: inlined redundant `is_index` intermediate binding (commit 057d448)

### Items Left Unfixed
- F7: `caddy_sentinel_e2e` compile error — pre-existing, not introduced by Slice 8.1; deferred to a cleanup task

## Slice 8.2 — Caddy-managed-paths ignore list

**Status:** complete
**Date:** 2026-05-10
**Summary:** Implemented the closed list of JSON pointers that Caddy mutates on its own and that the diff engine must discard. Covers TLS issuance state, upstream health caches, `automatic_https.disable_redirects` autopopulation, and `request_id` placeholders. Added regex-based pattern matching with static compilation at startup.

### Simplify Findings

Skipped (trivial slice).

### Items Fixed Inline

None — all acceptance criteria met on first attempt.

### Items Left Unfixed

None.

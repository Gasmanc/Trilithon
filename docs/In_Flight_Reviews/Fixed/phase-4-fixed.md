
## Slice 4.3 — 2026-05-03

- **Unnecessary public re-export of `double_option`** (`core/crates/core/src/model.rs`): Initially added `double_option` to the top-level model re-exports, which would have exposed an internal serde helper as part of the public API. Removed the re-export; consumers that need the helper should use it via the `primitive` sub-module path.

## Slice 4.4 — 2026-05-03

- **Redundant `.clone()` calls on last-use bindings** (`core/crates/core/src/model/desired_state.rs`, test): `route_id`, `up1_id`, `up2_id`, and the final use of `preset_id` were being cloned unnecessarily before being moved into `BTreeMap::insert`. Removed the redundant clones to satisfy `clippy::redundant_clone`.

## Slice 4.6 — 2026-05-03

- **Wildcard enum import** (`core/crates/core/src/mutation/types.rs`): `use Mutation::*` in both accessor methods replaced with fully-qualified `Self::` variant paths to satisfy `clippy::enum_glob_use`.
- **Non-const accessors** (`core/crates/core/src/mutation/types.rs`): `expected_version()` and `kind()` promoted to `const fn` per `clippy::missing_const_for_fn`.
- **Missing `# Errors` doc** (`core/crates/core/src/mutation/envelope.rs`): Added `# Errors` rustdoc section to `parse_envelope` to satisfy `clippy::missing_errors_doc`.
- **Trait-access style** (`core/crates/core/src/mutation/types.rs`): Test helper changed `Default::default()` to type-qualified calls (`MatcherSet::default()`, `HeaderRules::default()`) per `clippy::default_trait_access`.
- **`expect_used` in tests** (`types.rs`, `envelope.rs`): Added `#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic, clippy::disallowed_methods)]` on both test modules, consistent with the pattern used throughout the codebase.

## Slice 4.7 — 2026-05-03

- **`mod.rs` not allowed** (`core/crates/core/src/audit/mod.rs`): Moved to `core/crates/core/src/audit.rs` to satisfy the project's `clippy::mod_module_files` rule.
- **Wildcard import in `Display` impl** (`core/crates/core/src/audit.rs`): Replaced `use AuditEvent::*` with `Self::` prefixes on every match arm.
- **`serde_json::json!` macro expands to disallowed `unwrap`** (`core/crates/core/src/mutation/outcome.rs`): Replaced `json!(0)` and `json!(1)` with `serde_json::Value::Number(0.into())` and `serde_json::Value::Number(1.into())`.
- **Single-character string as pattern** (`core/crates/core/src/mutation/error.rs`): Changed `s.contains("9")` to `s.contains('9')`.

## Slice 4.8 — 2026-05-03

- **Identical match arms** (`core/crates/core/src/mutation/capability.rs`): `DetachPolicy` and `SetGlobalConfig` both returned `BTreeSet::new()` as separate arms — merged into a single arm to satisfy `clippy::match_same_arms`.
- **`option_if_let_else`** (`core/crates/core/src/mutation/capability.rs`): `match first_missing { None => Ok(()), Some(m) => Err(...) }` replaced with `first_missing.map_or(Ok(()), |m| Err(...))`.
- **Disallowed `unwrap` in production code** (`core/crates/core/src/mutation/capability.rs`): Initial algorithm used `missing.iter().next().unwrap()` — replaced with a single `.find()` iterator call so the `None` branch is handled by the combinator, eliminating the unwrap entirely.

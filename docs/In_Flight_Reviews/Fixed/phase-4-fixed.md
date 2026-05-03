
## Multi-review fix-pass — 2026-05-03

| Unit | Title | Type | Date | Commit |
|------|-------|------|------|--------|
| multi-review | DeleteUpstream can break referential integrity — crates/core/src/mutation/validate.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | Three-state patch semantics broken on RoutePatch and UpstreamPatch — crates/core/src/mutation/patches.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | UpdateRoute does not validate patched hostnames — crates/core/src/mutation/validate.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | UpgradePolicy accepts nonexistent target version — crates/core/src/mutation/validate.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | ValidationRule misuse: DuplicateRouteId for upstream collision and PolicyAttachmentMissing for missing routes — crates/core/src/mutation/validate.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | Hostname total-length check bypassed for wildcards — crates/core/src/model/route.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | unreachable!() in production code (apply.rs:109) — crates/core/src/mutation/apply.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | Duplicate AuditEvent enum — storage::types::AuditEvent collides with audit::AuditEvent — crates/core/src/storage/types.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | Duplicate UnixSeconds type alias in storage::types and model::primitive — crates/core/src/storage/types.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | proptest commutativity test checks membership only not full state equality — crates/core/tests/mutation_props.rs | Multi-review | 2026-05-03 | cf425a4 |
| multi-review | MutationKind missing schemars::JsonSchema derive — crates/core/src/mutation/types.rs | Multi-review | 2026-05-03 | cf425a4 |

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

## phase-end simplify — 2026-05-03

| Unit | Title | Type | Date | Commit |
|------|-------|------|------|--------|
| phase-end simplify | VOCAB const duplicates AUDIT_KINDS slice — core/crates/core/src/audit.rs | Simplify-skip | 2026-05-03 | — |
| phase-end simplify | check_hostnames_valid discards HostnameError details — core/crates/core/src/mutation/validate.rs | Simplify | 2026-05-03 | 23ba033 |
| phase-end simplify | Dead second s.len() > 253 check in validate_hostname — core/crates/core/src/model/route.rs | Simplify | 2026-05-03 | 23ba033 |
| phase-end simplify | serde_json::to_value(...).ok() repeated ~20× — extract helper — core/crates/core/src/mutation/apply.rs | Simplify | 2026-05-03 | 23ba033 |
| phase-end simplify | Policy mutation preamble copy-pasted 3× in apply.rs — core/crates/core/src/mutation/apply.rs | Simplify | 2026-05-03 | 23ba033 |
| phase-end simplify | Four inline route not found checks instead of calling check_route_exists — core/crates/core/src/mutation/validate.rs | Simplify | 2026-05-03 | 23ba033 |
| phase-end simplify | check_delete_upstream uses UpstreamReferenceMissing for wrong condition — core/crates/core/src/mutation/validate.rs | Simplify | 2026-05-03 | 23ba033 |
| phase-end simplify | capability.rs duplicates route-module derivation between CreateRoute and UpdateRoute — core/crates/core/src/mutation/capability.rs | Simplify-skip | 2026-05-03 | — |
| phase-end simplify | Rename caps_with_everything to empty_caps in proptest — core/crates/core/tests/mutation_props.rs | Simplify | 2026-05-03 | 23ba033 |

## Slice 4.9 — 2026-05-03

- **Function too long** (`apply.rs`, `validate.rs`): Both `apply_mutation`/`apply_variant` and `pre_conditions` exceeded clippy's 100-line limit — split into per-variant helper functions.
- **Infallible `Result` wrappers** (`apply.rs`): Seven helper functions returned `Result<Vec<DiffChange>, MutationError>` despite never failing — changed to return `Vec<DiffChange>` directly.
- **`similar_names` lint** (`apply.rs`): `patch`/`path` parameter names triggered clippy — renamed to `route_patch`/`upstream_patch` and `pointer`.
- **Unused test helper** (`apply.rs`): `state_with_upstream` was never called from any test — removed.
- **`const fn` opportunity** (`apply.rs`): `audit_event_for` takes only a `Copy` enum and has no heap operations — marked as `const fn`.
- **Format string style** (`validate.rs`): `format!("... '{}'", raw)` → `format!("... '{raw}'")`  to satisfy `clippy::uninlined_format_args`.
- **Redundant clones in tests** (`apply.rs`): `.clone()` on values not reused after the struct construction — removed.

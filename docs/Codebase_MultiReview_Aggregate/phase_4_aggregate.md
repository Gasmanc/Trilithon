# Phase 4 — Aggregate Review Plan

**Generated:** 2026-05-06T00:00:00Z
**Source:** `docs/In_Flight_Reviews/Findings/phase-4-findings.md` (consolidated multi-reviewer file)
**Reviewers:** gemini · codex · qwen · glm · minimax · kimi · phase-end-simplify
**Raw findings:** ~64 across 6 reviewers + simplify pass
**Unique findings:** 29 actionable after clustering
**Consensus:** 0 unanimous · 2 majority · 27 single-reviewer
**Conflicts:** 0
**Superseded (already fixed):** 34 (32 fixed by cf425a4/23ba033 + 2 simplify-skips)

---

## How to use this document

Feed this file to `/review-remediate` to drive the fix cycle. Each finding has a
unique ID (F001, F002, …) that `review-remediate` uses to track state. Do not
renumber or delete findings — append `SUPERSEDED` status instead.

---

## CRITICAL Findings

*(None — all CRITICAL findings from this phase were resolved in the multi-review fix-pass at commit cf425a4.)*

---

## HIGH Findings

### F001 · [HIGH] ImportFromCaddyfile bypasses precondition validation
**Consensus:** MAJORITY · flagged by: codex (HIGH), gemini (WARNING)
**File:** `crates/core/src/mutation/validate.rs` · **Lines:** 75-78
**Description:** `Mutation::ImportFromCaddyfile` has no precondition checks in `pre_conditions`. Crafted payloads can introduce invalid hostnames, duplicate IDs, and dangling upstream references into `DesiredState`, violating core domain invariants.
**Suggestion:** Validate imported routes and upstreams with the same checks used by the create/update flows — run hostname validation, duplicate-ID checks, and upstream-reference checks on every entity in the `ParsedCaddyfile`.
**Claude's assessment:** Agree strongly. Every other mutation variant has explicit pre-conditions; the absence here is a real invariant gap, not a feature gap. Priority fix.

---

### F002 · [HIGH] DesiredState.policies is dead state — never populated
**Consensus:** SINGLE · flagged by: kimi
**File:** `crates/core/src/model/desired_state.rs` · **Lines:** 28-29
**Description:** `policies: BTreeMap<PolicyId, PolicyAttachment>` is declared in `DesiredState` but no mutation handler ever inserts, reads, or removes entries. `AttachPolicy` / `DetachPolicy` / `UpgradePolicy` all operate on per-route `policy_attachment` fields only. The dead field will affect canonical hashing in Phase 5.
**Suggestion:** Either wire `AttachPolicy` to populate `DesiredState.policies` (if the intent is a top-level registry), or remove the field until a real owner arrives.
**Claude's assessment:** Agree. This was explicitly deferred from the multi-review fix-pass ("design decision deferred to Phase 5"). It should be resolved at the start of Phase 5 before any hashing work begins. Including here to ensure it is tracked.

---

### F003 · [HIGH] PolicyAttachment and RoutePolicyAttachment are structural duplicates
**Consensus:** SINGLE · flagged by: kimi
**File:** `crates/core/src/model/policy.rs` · **Lines:** 22-28
**Description:** Two types (`PolicyAttachment` and `RoutePolicyAttachment`) have identical fields and semantics but live in different modules with different names.
**Suggestion:** Consolidate to one canonical type. If semantics genuinely differ, document the distinction clearly in both type doc comments.
**Claude's assessment:** Partially agree. The multi-review fix-pass deferred this because "one represents a resolved attachment in DesiredState.policies, the other is embedded in Route." Given F002 (policies field is dead), this evaluation is still blocked. Defer resolution of F003 until F002 is resolved.

---

## WARNING Findings

### F004 · [WARNING] ImportFromCaddyfile emits coarse whole-map diffs and silently overwrites entities
**Consensus:** MAJORITY · flagged by: gemini (WARNING), kimi (WARNING)
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 325-354
**Description:** `apply_import_caddyfile` serialises entire `routes` and `upstreams` maps before/after, producing two coarse `DiffChange` entries. Existing entities with the same ID are silently overwritten. There is no per-entity diff and no collision policy.
**Suggestion:** Emit per-entity `DiffChange` entries for each route/upstream that is actually added or modified. Document the collision policy (reject duplicates vs. overwrite).
**Claude's assessment:** Agree. Coarse diffs make audit events useless and silent overwriting is dangerous once the config HTTP API is live.

---

### F005 · [WARNING] apply_set_global_config and apply_set_tls_config emit no-op diffs
**Consensus:** SINGLE · flagged by: kimi
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 273-323
**Description:** Both functions unconditionally push a `DiffChange` even when the patch made no actual changes (before == after), polluting the audit log with spurious no-op events.
**Suggestion:** Compare before/after and skip `DiffChange` emission when values are identical.
**Claude's assessment:** Agree. Easy fix with a before/after equality check.

---

### F006 · [WARNING] Identifier newtypes accept any string — no ULID validation
**Consensus:** SINGLE · flagged by: kimi
**File:** `crates/core/src/model/identifiers.rs` · **Lines:** 8-55
**Description:** `RouteId`, `UpstreamId`, `PolicyId`, etc. all expose `pub String` inner fields and derive `Deserialize` with no validation. Any string is accepted as a valid ID at deserialization time, undermining the newtype guarantee.
**Suggestion:** Make the inner field private, add a checked constructor (`from_str` or `TryFrom<String>`), or at minimum update the doc comments to document the type as "opaque string identifier" rather than "ULID".
**Claude's assessment:** Agree. The newtype is misleadingly named if it accepts arbitrary strings. At minimum downgrade the docs; ideally add a `TryFrom` constructor.

---

### F007 · [WARNING] schemars lives in core [dependencies] rather than gated as build/feature dep
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/Cargo.toml` · **Lines:** 13
**Description:** `schemars = "0.8"` is in main `[dependencies]`, introducing a compile-time dep for all consumers of `trilithon-core` even when they don't need schema generation.
**Suggestion:** Gate behind a cargo feature flag (e.g. `features = ["schema"]`) or move to `[dev-dependencies]` if only used by the `gen_mutation_schemas` binary.
**Claude's assessment:** Agree in principle, but check whether the derives (`#[derive(JsonSchema)]`) are conditionally compiled or always present — if always present, a feature flag requires conditional derives throughout.

---

### F008 · [WARNING] Hardcoded relative path `../../../docs/schemas/mutations` in build.rs
**Consensus:** MAJORITY · flagged by: minimax (WARNING), kimi (SUGGESTION)
**File:** `crates/core/build.rs` · **Lines:** 12-16
**Description:** The output path for schema generation is a hardcoded relative path from the crate root. This breaks if the crate is ever moved, vendored, or built from a different working directory.
**Suggestion:** Resolve the workspace root via `cargo metadata` or use `CARGO_WORKSPACE_DIR` (available in recent Cargo). Then construct the path dynamically.
**Claude's assessment:** Agree. Minor but worth fixing before other teams use this crate.

---

### F009 · [WARNING] gen_mutation_schemas uses process::exit(1) — bypasses drop semantics
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/src/bin/gen_mutation_schemas.rs` · **Lines:** 99
**Description:** `std::process::exit(1)` is called on error, skipping Rust's normal drop/cleanup path.
**Suggestion:** Return an `ExitCode` from `main` or propagate a `Result<(), Box<dyn Error>>` and let the runtime handle the exit.
**Claude's assessment:** Agree. Easy refactor — `fn main() -> Result<(), Box<dyn Error>>` is idiomatic and compatible with the existing error path.

---

### F010 · [WARNING] Proptest suite only covers CreateRoute — missing 12 other mutation variants
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/tests/mutation_props.rs` · **Lines:** general
**Description:** The three property tests (determinism, commutativity, version postcondition) only exercise `CreateRoute`. The remaining 12 mutation variants have no property test coverage.
**Suggestion:** Extend the `arb_mutation` generator to cover all 13 variants and run the same three properties against all of them.
**Claude's assessment:** Agree. Property tests are most valuable when they cover the full variant space.

---

### F011 · [WARNING] All-None GlobalConfigPatch accepted as silent no-op
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/src/model/global.rs` · **Lines:** 59
**Description:** A `SetGlobalConfig` mutation where all patch fields are `None` is accepted and produces a diff, with no error or warning. This is a valid but confusing API state.
**Suggestion:** Either reject all-None patches with a `ValidationRule::NoOpMutation` error, or explicitly document the all-None case as a no-op and suppress the diff emission.
**Claude's assessment:** Agree. Ties into F005 — if no-op diffs are suppressed (F005), an all-None patch would silently succeed with an empty diff. Recommend rejecting in pre-conditions instead.

---

### F012 · [WARNING] build.rs missing `cargo:rerun-if-changed` for schema output directory
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/build.rs` · **Lines:** 20
**Description:** The build script doesn't emit `cargo:rerun-if-changed=docs/schemas/mutations/` as an input, so Cargo won't rerun the build script when schema files change — stale builds won't detect drift.
**Suggestion:** Add `println!("cargo:rerun-if-changed=docs/schemas/mutations/");` (or point to the source types instead).
**Claude's assessment:** Agree. The existing `check-schemas` recipe in `Justfile` catches drift at gate-run time but incremental builds won't.

---

### F013 · [WARNING] apply_mutation error variants carry no payload — debugging is hard
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/src/mutation/error.rs` · **Lines:** general
**Description:** Several `MutationError` variants (e.g. `Forbidden`) carry no context fields, making it difficult to diagnose why a mutation was rejected without re-running with a debugger.
**Suggestion:** Enrich key variants with context fields — for `Forbidden`, include the offending IDs and the reason; for `ValidationFailed`, propagate the failing rule plus the JSON pointer of the offending field.
**Claude's assessment:** Agree. At minimum `Forbidden { reason, mutation_id }` would make production debugging viable.

---

### F014 · [WARNING] Per-variant schema stub files lack `$id` field
**Consensus:** SINGLE · flagged by: minimax
**File:** `docs/schemas/mutations/*.json` · **Lines:** general
**Description:** The 14 per-variant stub schema files have no `$id` field, making them non-resolvable in a JSON Schema registry.
**Suggestion:** Add `"$id": "https://trilithon.internal/schemas/mutations/<VariantName>.json"` (or a relative URI) to each stub.
**Claude's assessment:** Agree if these schemas will be served or referenced externally. Low priority if they're only used for local tooling.

---

### F015 · [WARNING] Per-variant schema stub `$ref` points to a non-existent definition
**Consensus:** SINGLE · flagged by: codex
**File:** `crates/core/src/bin/gen_mutation_schemas.rs` · **Lines:** 74-78
**Description:** Stub schemas reference `Mutation.json#/definitions/Mutation`, but `Mutation.json` does not define a `definitions.Mutation` key — the union is top-level.
**Suggestion:** Point `$ref` to `Mutation.json` directly (no fragment) or to `Mutation.json#` to reference the root schema object.
**Claude's assessment:** Agree. Invalid `$ref` targets will cause any JSON Schema validator to report a broken reference.

---

## SUGGESTION / LOW Findings

### F016 · [SUGGESTION] check_upstreams_exist error path omits the offending upstream's index
**Consensus:** SINGLE · flagged by: gemini
**File:** `crates/core/src/mutation/validate.rs` · **Lines:** 91-103
**Description:** When `check_upstreams_exist` fails, the `JsonPointer` in the error points to the `upstreams` collection but not to the specific index that was missing, making client-side error display imprecise.
**Suggestion:** Include the index of the offending upstream ID in the `JsonPointer` (e.g. `/upstreams/0`).
**Claude's assessment:** Agree. Low effort, improves API ergonomics.

---

### F017 · [SUGGESTION] Misleading error for empty hostname labels — returns HyphenBoundary
**Consensus:** SINGLE · flagged by: gemini
**File:** `crates/core/src/model/route.rs` · **Lines:** 136-140
**Description:** A hostname with an empty label (e.g. `example..com`) returns `HostnameError::HyphenBoundary` instead of something like `EmptyLabel`. The error is technically coincidental, not semantically correct.
**Suggestion:** Add an early `if label.is_empty()` check and return a dedicated `EmptyLabel` error variant.
**Claude's assessment:** Agree. Clear error messaging is worth the small addition.

---

### F018 · [SUGGESTION] Test `idempotency_on_mutation_id` verifies determinism, not idempotency
**Consensus:** SINGLE · flagged by: qwen
**File:** `crates/core/tests/mutation_props.rs` · **Lines:** 72-98
**Description:** The test name says "idempotency" but it checks that repeated application of the same mutation with the same ID produces the same result — that's determinism.
**Suggestion:** Rename to `apply_mutation_is_deterministic`.
**Claude's assessment:** Agree. Trivial rename; avoids conceptual confusion when true idempotency tests are added later.

---

### F019 · [SUGGESTION] Outdated comment in build.rs references wrong path
**Consensus:** SINGLE · flagged by: qwen
**File:** `crates/core/build.rs` · **Lines:** 8
**Description:** Comment references a path for `CARGO_MANIFEST_DIR` that doesn't match the actual path used.
**Suggestion:** Update the comment to match the actual path resolution in the script.
**Claude's assessment:** Agree. Overlaps with F008 — fix both in the same pass.

---

### F020 · [SUGGESTION] `#[allow(clippy::option_option)]` is struct-level, not field-level
**Consensus:** SINGLE · flagged by: qwen
**File:** `crates/core/src/mutation/patches.rs` · **Lines:** 26, 64
**Description:** The suppression attribute covers the entire struct but only a subset of fields actually use `Option<Option<T>>`. This broadens the suppression scope unnecessarily.
**Suggestion:** Move the `#[allow]` to the individual double-Option fields.
**Claude's assessment:** Agree with the principle. Check CLAUDE.md format: suppression must include `zd:<id> expires:<YYYY-MM-DD> reason:<short>`.

---

### F021 · [SUGGESTION] Suppression comment in capability.rs missing standard zd: format
**Consensus:** SINGLE · flagged by: qwen
**File:** `crates/core/src/mutation/capability.rs` · **Lines:** 103
**Description:** The `// zd:CAP-PRESET` comment doesn't include the required `expires:` and `reason:` fields mandated by CLAUDE.md suppression format.
**Suggestion:** Expand to `// zd:CAP-PRESET expires:2026-12-31 reason:<short description>`.
**Claude's assessment:** Agree. Policy compliance, low effort.

---

### F022 · [SUGGESTION] gen_mutation_schemas binary lives in core crate — blurs three-layer boundary
**Consensus:** SINGLE · flagged by: glm
**File:** `crates/core/src/bin/gen_mutation_schemas.rs` · **Lines:** general
**Description:** The `gen_mutation_schemas` binary performs I/O (writes files) but lives inside `crates/core/`, which the CLAUDE.md architecture requires to be pure logic with no I/O.
**Suggestion:** Move the binary to `crates/cli/` or a dedicated `xtask` crate.
**Claude's assessment:** Agree with the architectural concern. However, this binary needs the `JsonSchema` derives from `core` types — moving it to `cli` is the simplest path. Note that F007 (schemars as feature-gated dep) should be resolved first or in tandem.

---

### F023 · [SUGGESTION] RoutePatch doc comment inaccurately describes triple-state pattern
**Consensus:** SINGLE · flagged by: glm
**File:** `crates/core/src/mutation/patches.rs` · **Lines:** 29-35
**Description:** The `RoutePatch` doc comment implies all fields follow the triple-state `Option<Option<T>>` convention, but several fields use single `Option<T>`.
**Suggestion:** Update the doc comment to explicitly enumerate which fields are triple-state (absent/clear/set) and which are dual-state (absent/set).
**Claude's assessment:** Agree. Important for API consumers who need to understand the semantics.

---

### F024 · [SUGGESTION] Schema stubs use non-standard `x-variant` extension field
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/src/bin/gen_mutation_schemas.rs` · **Lines:** 91
**Description:** Stub schemas include an `x-variant` vendor extension field that is not part of any JSON Schema standard. Tooling may ignore or reject it.
**Suggestion:** Remove `x-variant`; the `title` field already identifies the variant. Alternatively standardise on a documented vendor extension convention.
**Claude's assessment:** Weak agree. If the field serves no tooling purpose, remove it. If it's used by internal tooling, document it.

---

### F025 · [SUGGESTION] DesiredState::empty() is an alias for Default::default() — unnecessary API surface
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/src/model/desired_state.rs` · **Lines:** 37
**Description:** `DesiredState::empty()` unconditionally delegates to `Self::default()` with no semantic distinction.
**Suggestion:** Remove `empty()` and have callers use `DesiredState::default()` directly.
**Claude's assessment:** Partially agree. `empty()` is more semantically expressive in application code than `default()`. If the intent is "empty configuration snapshot," the name carries meaning. Low priority.

---

### F026 · [SUGGESTION] AuditEvent has both a derived Debug and a manual impl Debug
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/src/audit.rs` · **Lines:** 72
**Description:** `AuditEvent` derives `Debug` but also has a manual `impl fmt::Debug` block, causing a compile error or shadowing (the manual impl wins). This is dead code at best, a mistake at worst.
**Suggestion:** Remove whichever is redundant — the manual impl or the derive.
**Claude's assessment:** Agree. This should be a compiler error or dead derive; either way fix it.

---

### F027 · [SUGGESTION] Dead `audit_event_for` arm for Rollback should be annotated
**Consensus:** SINGLE · flagged by: kimi
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 463-480
**Description:** The `Rollback` arm of `audit_event_for` is unreachable in Phase 4 because `pre_conditions` always rejects `Rollback`. There is no comment explaining this.
**Suggestion:** Add `// reachable in Phase 7: Rollback support` or a tracked suppression comment so future maintainers understand why the arm exists.
**Claude's assessment:** Agree. Minimal effort, prevents confusion.

---

### F028 · [SUGGESTION] MutationOutcome.kind field should be renamed audit_event
**Consensus:** SINGLE · flagged by: kimi
**File:** `crates/core/src/mutation/outcome.rs` · **Lines:** 6-15
**Description:** The field `kind: AuditEvent` uses the name `kind`, which is conventionally used for discriminants/enums. It carries a full `AuditEvent` value, causing naming confusion with `MutationKind`.
**Suggestion:** Rename to `audit_event: AuditEvent`.
**Claude's assessment:** Agree. The rename would disambiguate `MutationKind` (which mutation type) from `AuditEvent` (what happened as a result).

---

### F029 · [SUGGESTION] Diff serialization uses .ok() — errors silently produce None
**Consensus:** SINGLE · flagged by: kimi
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** general
**Description:** Although the phase-end simplify extracted a `to_json<T>` helper (commit 23ba033), the helper internally uses `.ok()`, so serialization failures silently produce `None` in the diff's `before`/`after` fields rather than propagating an error.
**Suggestion:** Propagate serialization errors, or use `.expect("type T is always JSON-serializable")` with a load-bearing message if the type is known to be infallible.
**Claude's assessment:** Agree. `.ok()` silently swallowing errors violates the project's no-silent-suppressions rule. If `serde_json::to_value` on these types can actually fail, we need to know.

---

## CONFLICTS

*(No unresolved conflicts — all reviewer disagreements in this phase were resolved by the multi-review fix-pass.)*

---

## Out-of-scope / Superseded

Findings excluded from the actionable list:

| ID | Title | Reason |
|----|-------|--------|
| — | **gemini** Hostname length limit bypass | Fixed cf425a4 |
| — | **gemini** Incorrect ValidationRule for UpstreamId | Fixed cf425a4 |
| — | **codex** DeleteUpstream referential integrity | Fixed cf425a4 |
| — | **codex** Option<Option<T>> patch clear semantics | Fixed cf425a4 |
| — | **codex** UpdateRoute does not validate patched hostnames | Fixed cf425a4 |
| — | **codex** UpgradePolicy accepts nonexistent target version | Fixed cf425a4 |
| — | **codex** ValidationRules misclassified (DuplicateRouteId + PolicyAttachmentMissing) | Fixed cf425a4 |
| — | **qwen** Wrong ValidationRule for duplicate upstream id | Fixed cf425a4 |
| — | **qwen** Wrong ValidationRule for missing route | Fixed cf425a4 |
| — | **qwen** unreachable!() in production code | Fixed cf425a4 |
| — | **qwen** Incorrect Option<Option<T>> patch semantics in TLS | Fixed cf425a4 |
| — | **glm** unreachable! in production code path | Fixed cf425a4 |
| — | **glm** Duplicate AuditEvent enum | Fixed cf425a4 |
| — | **glm** Duplicate UnixSeconds type alias | Fixed cf425a4 |
| — | **glm** ValidationRule::PolicyAttachmentMissing overloaded | Fixed cf425a4 |
| — | **glm** ValidationRule::DuplicateRouteId for upstream | Fixed cf425a4 |
| — | **glm** UpdateRoute patch no hostname validation | Fixed cf425a4 |
| — | **glm** DeleteUpstream orphan route references | Fixed cf425a4 |
| — | **glm** MutationKind missing schemars::JsonSchema | Fixed cf425a4 |
| — | **kimi** Three-state patch semantics broken (RoutePatch/UpstreamPatch) | Fixed cf425a4 |
| — | **kimi** ValidationRule misuse: PolicyAttachmentMissing | Fixed cf425a4 |
| — | **kimi** ValidationRule misuse: DuplicateRouteId | Fixed cf425a4 |
| — | **kimi** Hostname total-length bypass for wildcards | Fixed cf425a4 |
| — | **kimi** proptest commutativity checks membership only | Fixed cf425a4 |
| — | **kimi** storage::types::AuditEvent / audit::AuditEvent collision | Fixed cf425a4 |
| — | **phase-end** check_hostnames_valid discards HostnameError details | Fixed 23ba033 |
| — | **phase-end** Dead second s.len() > 253 check in validate_hostname | Fixed 23ba033 |
| — | **phase-end** serde_json::to_value(..).ok() repeated ~20× (extracted to_json helper) | Fixed 23ba033 |
| — | **phase-end** Policy mutation preamble copy-pasted 3× | Fixed 23ba033 |
| — | **phase-end** Four inline route-not-found checks vs check_route_exists | Fixed 23ba033 |
| — | **phase-end** check_delete_upstream wrong ValidationRule | Fixed 23ba033 |
| — | **phase-end** Rename caps_with_everything → empty_caps | Fixed 23ba033 |
| — | **phase-end** VOCAB const duplicates AUDIT_KINDS slice | Skipped — `audit_vocab::AUDIT_KINDS` does not exist in Phase 4; deferred until storage module is introduced |
| — | **phase-end** capability.rs route-module derivation duplicated | Skipped — only 2 call sites, three-use rule not met per project conventions |

---

## Summary statistics

| Severity | Unanimous | Majority | Single | Total |
|----------|-----------|----------|--------|-------|
| CRITICAL | 0 | 0 | 0 | 0 |
| HIGH | 0 | 1 | 2 | 3 |
| WARNING | 0 | 1 | 11 | 12 |
| SUGGESTION | 0 | 0 | 14 | 14 |
| **Total** | **0** | **2** | **27** | **29** |

---

## Re-run: 2026-05-06T12:00:00Z

**New source:** `docs/End_of_Phase_Reviews/Findings/` — 10 end-of-phase reviewer files
**Reviewers:** code_adversarial · codex · gemini · glm · kimi (API error, no output) · learnings_match · minimax · qwen · scope_guardian · security
**Raw findings:** ~57 across 9 producing reviewers
**New unique findings:** 33 (F030–F062) after deduplication against F001–F029
**Consensus upgrades on prior findings:** 4 (see "Prior Finding Updates" section below)
**Newly superseded in this run:** 12 (duplicates of F001–F029 or already-fixed patterns)

---

### Prior Finding Updates (existing IDs, upgraded consensus/severity)

| ID | Prior status | Update |
|----|-------------|--------|
| F001 | MAJORITY (2/6) | **UNANIMOUS** — all 9 new reviewers that produced output flagged ImportFromCaddyfile validation gap |
| F004 | MAJORITY | **UNANIMOUS** — code_adversarial, codex, minimax, qwen all independently flag the silent-overwrite / coarse-diff pair |
| F006 | SINGLE · WARNING | **MAJORITY · CRITICAL** — qwen escalates to CRITICAL (arbitrary string injection into storage keys); security corroborates at WARNING |
| F023 | SINGLE · SUGGESTION | **MAJORITY · WARNING** — glm (WARNING) and qwen (HIGH) both flag; take WARNING |
| F029 | SINGLE · SUGGESTION | **MAJORITY · WARNING** — qwen (WARNING) and security (WARNING) both flag; take WARNING |

---

### CRITICAL Findings (this run)

### F030 · [CRITICAL] schema_drift integration test missing — CI cannot detect schema drift
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `crates/core/tests/` · **Lines:** general
**Description:** Slice 4.10 (the phase TODO) explicitly requires a `schema_drift.rs` integration test with a `schemas_match_committed` test that runs `gen_mutation_schemas` and asserts `git diff --exit-code docs/schemas/mutations/` is clean. The file does not exist. Only `mutation_props.rs` is present. The existing `check-schemas` recipe in `Justfile` only runs at manual gate-time; without the integration test, schema drift is undetected in incremental CI runs.
**Suggestion:** Create `crates/core/tests/schema_drift.rs` with a `#[test] fn schemas_match_committed()` that invokes `gen_mutation_schemas` via `std::process::Command` and asserts the exit code is 0 and `git diff --exit-code docs/schemas/mutations/` reports no changes.
**Claude's assessment:** Agree strongly. This is a spec gap — the TODO required the test. Missing drift detection is a process invariant failure.

---

### HIGH Findings (this run)

### F031 · [HIGH] UpgradePolicy / SetGlobalConfig / SetTlsConfig snapshot "before" from new_state, not original state
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 182-196, 238-257, 264, 287
**Description:** `policy_attachment_preamble`, `apply_set_global_config`, and `apply_set_tls_config` all read the `before` snapshot from `new_state` (the already-cloned mutable copy) rather than from the original immutable `state`. This is coincidentally correct today because `new_state` hasn't yet been mutated when the preamble runs — but the dependency on call ordering is invisible in the type system. Any future refactor that reorders operations within `apply_variant`, or a compound mutation that shares `new_state`, will silently record an incorrect `before`, making the audit diff appear as a no-op. Compare: `apply_route_patch` and `apply_upstream_patch` correctly pass the immutable `state` for the before read.
**Suggestion:** Pass the original immutable `state: &DesiredState` into `policy_attachment_preamble`, `apply_set_global_config`, and `apply_set_tls_config` and read `before` from it, matching the pattern used by the patch helpers.
**Claude's assessment:** Agree. The ordering dependency is a latent bug that will be invisible until a compound mutation phase breaks it. Fix now while the blast radius is small.

---

### F032 · [HIGH] apply_upgrade_policy silently no-ops when policy_attachment is None — self-defense gap
**Consensus:** MAJORITY · flagged by: code_adversarial (HIGH), qwen (WARNING)
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 238-257
**Description:** `apply_upgrade_policy` uses `if let Some(attachment) = route.policy_attachment.as_mut()` — if `policy_attachment` is `None`, the if-let arm is silently skipped and the function returns `Ok(vec![DiffChange { before: None, after: None }])` while the version counter still increments. `check_upgrade_policy` in `pre_conditions` guards against this, but the two layers are structurally decoupled: `apply_upgrade_policy` can be called directly (e.g., a Phase 7 rollback path), producing a silent no-op mutation with a real version increment and zero state change — corrupting audit history.
**Suggestion:** Replace the `if let Some(attachment)` with `.ok_or_else(|| MutationError::Forbidden { reason: ForbiddenReason::PolicyAttachmentMissing })` so the apply layer is self-defending regardless of whether validation ran first.
**Claude's assessment:** Agree. Apply helpers must be safe to call in isolation. The validation layer cannot be the only guard.

---

### F033 · [HIGH] Route.updated_at is never updated by patch mutations
**Consensus:** SINGLE · flagged by: gemini
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 236-271
**Description:** `Route` has an `updated_at: UnixSeconds` field but `apply_route_patch` only applies fields present in `RoutePatch`. `apply_mutation` is pure and takes no timestamp argument, and `RoutePatch` has no `updated_at` field. The result: every `UpdateRoute` mutation leaves `updated_at` unchanged at the route's creation time, making the timestamp meaningless after the first patch.
**Suggestion:** Either add a `now: UnixSeconds` parameter to `apply_mutation` (making it non-pure but accurate), or remove `updated_at` from `Route` until a Phase 5+ persistence layer can supply a real timestamp at write time.
**Claude's assessment:** Agree — this is a real semantic bug. The cleanest fix is to remove `updated_at` from the pure core model and add it in the adapter layer when persisting. Adding a timestamp arg to `apply_mutation` would break purity and make property tests harder.

---

### F034 · [HIGH] content_address SHA-256 utility is out of scope in mutation/types.rs
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `crates/core/src/mutation/types.rs` · **Lines:** 224-248
**Description:** `content_address(canonical_json_bytes: &[u8]) -> String` and a corresponding `sha2` dependency are not mentioned in any Phase 4 slice. The function computes a SHA-256 content address — that operation belongs to the snapshot writer in Phase 5 (ADR-0009 §6.5). Placing it here pre-empts Phase 5's design and bleeds snapshot-layer concerns into the mutation-type file, also violating the no-I/O-in-core constraint if the hash is used for storage addressing.
**Suggestion:** Remove `content_address` and the `sha2` dependency from `mutation/types.rs`. Move to `crates/core/src/snapshot.rs` (or a new `hash.rs` utility) when Phase 5 is implemented.
**Claude's assessment:** Agree. Out-of-scope additions in a strict phased plan create hidden coupling. Remove now.

---

### WARNING Findings (this run)

### F035 · [WARNING] DesiredState.version is a public field — callers can bypass the +1 invariant
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `crates/core/src/model/desired_state.rs` · **Lines:** general
**Description:** `DesiredState.version` is `pub`, so any code that clones a `DesiredState` can directly write `state.version = N`, bypassing the conflict-check invariant enforced by `apply_mutation`. In Phase 7, a rollback implementation that directly sets `new_state.version` to match a snapshot's `config_version` would bypass the monotonic `+1` invariant without any compile-time warning.
**Suggestion:** Make `version` a private field exposed through a read accessor only, or add a post-condition assertion in `apply_mutation` that `new_state.version == state.version + 1` before returning `Ok`.
**Claude's assessment:** Agree. The field should be opaque to callers; `apply_mutation` is the only legal writer.

---

### F036 · [WARNING] Version increment uses unchecked i64 arithmetic — overflow path exists
**Consensus:** MAJORITY · flagged by: code_adversarial (WARNING), qwen (SUGGESTION)
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 55
**Description:** `new_state.version = state.version + 1` uses the default `+` operator on `i64`. In release builds, overflow wraps silently (or panics with debug assertions). If the version wraps, the conflict check (`expected != state.version`) becomes unreliable — a wrapped version may coincidentally match a stale in-flight mutation's `expected_version`, allowing that mutation to be accepted as current. The proptest version range `0..=100` doesn't cover this edge case.
**Suggestion:** Use `state.version.checked_add(1).ok_or(MutationError::Forbidden { reason: ForbiddenReason::VersionOverflow })` and add `VersionOverflow` to `ForbiddenReason`.
**Claude's assessment:** Agree. The overflow scenario is remote but the fix costs one arithmetic check per mutation. The correct error path is cheap insurance.

---

### F037 · [WARNING] UpgradePolicy accepts preset_version = 0 — no lower-bound check on AttachPolicy
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `crates/core/src/mutation/validate.rs` · **Lines:** 201-249
**Description:** `check_upgrade_policy` validates `to_version > attachment.preset_version` and that the preset registry matches `to_version`, but `preset_version: 0` on `AttachPolicy` is never validated — there is no lower-bound check on the `u32`. A `PresetVersion { version: 0 }` in `state.presets` anchors the lifecycle at version zero ("no preset yet"), and a subsequent `UpgradePolicy { to_version: 1 }` passes all checks. Version 0 has no formal definition in the schema.
**Suggestion:** Add `PolicyPresetVersionZero` validation rule and reject `preset_version == 0` in `check_attach_policy`. Mirror in `check_upgrade_policy`. Document in `PresetVersion` that versions start at 1.
**Claude's assessment:** Agree. Zero is an ambiguous sentinel; define the invariant explicitly.

---

### F038 · [WARNING] justfile missing check-schemas recipe — docs reference a non-existent target
**Consensus:** SINGLE · flagged by: codex
**File:** `docs/schemas/mutations/README.md` · **Lines:** 14-18
**Description:** The README instructs users to run `just check-schemas` but the `Justfile` has no `check-schemas` recipe. This breaks the documented drift-check workflow and makes it easy to miss schema drift.
**Suggestion:** Add a `check-schemas` recipe (run `gen_mutation_schemas` then `git diff --exit-code docs/schemas/mutations/`) or update the README to the real command.
**Claude's assessment:** Agree. Closely related to F030 (missing schema_drift test). Fix both together.

---

### F039 · [WARNING] Malformed JSON envelope misclassified as MissingExpectedVersion
**Consensus:** MAJORITY · flagged by: codex (WARNING), minimax (WARNING)
**File:** `crates/core/src/mutation/envelope.rs` · **Lines:** 59-67
**Description:** `parse_envelope` checks `mutation_val.get("expected_version")` without first verifying that `mutation` is a JSON object. Non-object payloads (strings, arrays, numbers) will be reported as `EnvelopeError::MissingExpectedVersion` instead of `EnvelopeError::Malformed`, producing incorrect rejection classification. Clients receiving `MissingExpectedVersion` will add a version field to a structurally invalid payload rather than fixing the payload structure.
**Suggestion:** Validate that `mutation` is a JSON object first; only emit `MissingExpectedVersion` when the object is valid but the key is absent.
**Claude's assessment:** Agree. Incorrect error classification misleads API consumers.

---

### F040 · [WARNING] TLS capability gating only checks email field — other TLS fields ungated
**Consensus:** MAJORITY · flagged by: gemini (WARNING), glm (WARNING)
**File:** `crates/core/src/mutation/capability.rs` · **Lines:** 104-118
**Description:** `referenced_caddy_modules` for `SetTlsConfig` only requires the `tls` Caddy module when `patch.email` is `Some(Some(_))`. Setting `on_demand_enabled`, `on_demand_ask_url`, `default_issuer` (including `TlsIssuer::Acme`), or any other TLS field does not trigger a capability check. These operations also require the `tls` application module in Caddy.
**Suggestion:** Gate on any non-`None` TLS patch field — require the `tls` module whenever any field in `TlsConfigPatch` is being set/cleared.
**Claude's assessment:** Agree. The email-only gate is an implementation gap; any non-trivial TLS configuration requires the module.

---

### F041 · [WARNING] Incorrect Caddy module for request header rules — rewrite vs. headers
**Consensus:** SINGLE · flagged by: gemini
**File:** `crates/core/src/mutation/capability.rs` · **Lines:** 32, 45
**Description:** `referenced_caddy_modules` maps `route.headers.request` to `http.handlers.rewrite`. In Caddy, `rewrite` is for URI manipulation; header set/add/delete operations are handled by `http.handlers.headers`. This incorrect mapping will cause `check_capabilities` to approve a route whose header rules require the `headers` handler module while only checking for the `rewrite` module.
**Suggestion:** Map both `request` and `response` header rules to `http.handlers.headers`.
**Claude's assessment:** Agree. This is a factual module-name error that will produce incorrect capability checks once the Caddy apply layer is wired.

---

### F042 · [WARNING] PolicyPresetMissing reused for version-mismatch errors
**Consensus:** MAJORITY · flagged by: glm (WARNING), qwen (WARNING)
**File:** `crates/core/src/mutation/validate.rs` · **Lines:** 172-179, 233
**Description:** When `check_attach_policy` or `check_upgrade_policy` finds that the preset exists but the requested `preset_version` doesn't match the current version, the error uses `ValidationRule::PolicyPresetMissing`. The preset exists; only the version is wrong. This misuse obscures the actual problem for API consumers and conflates two distinct failure modes.
**Suggestion:** Add `ValidationRule::PolicyPresetVersionMismatch` and use it for all version-mismatch paths in both `check_attach_policy` and `check_upgrade_policy`.
**Claude's assessment:** Agree. Correct error taxonomy is API contract — callers cannot distinguish "preset doesn't exist" from "preset exists at wrong version" with the current code.

---

### F043 · [WARNING] Redundant get-after-check_route_exists dead paths in validate.rs
**Consensus:** MAJORITY · flagged by: glm (WARNING, check_upgrade_policy), qwen (WARNING, check_detach_policy)
**File:** `crates/core/src/mutation/validate.rs` · **Lines:** 207-214 (check_upgrade_policy), 185-198 (check_detach_policy)
**Description:** Two patterns in `validate.rs` call `check_route_exists(state, route_id)?` and then immediately call `state.routes.get(route_id)` again with an `ok_or_else` or `if let Some`. The second get is either unreachable (check_upgrade_policy's `ok_or_else`) or creates a dead code path that silently returns `Ok(())` if the inner `if let Some` doesn't match (check_detach_policy). Both are fragile if `check_route_exists` is ever removed or reordered.
**Suggestion (check_upgrade_policy):** Remove the redundant `ok_or_else`; after `check_route_exists` succeeds, use `.expect("guaranteed by check_route_exists")` or restructure to avoid the second lookup.
**Suggestion (check_detach_policy):** Replace `if let Some(route)` with `.ok_or_else(|| ...)` to make the no-attachment guard unconditional and not dependent on the inner `if let`.
**Claude's assessment:** Agree on both. The duplication is a maintenance hazard.

---

### F044 · [WARNING] audit_vocab VOCAB const is private — Phase 6 cannot import it
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `crates/core/src/audit.rs` · **Lines:** 163-207
**Description:** The TODO (Slice 4.7) specifies importing from `core::storage::audit_vocab`, but the implementation embeds `VOCAB` as a `const` inside `#[cfg(test)]`. This makes the vocabulary list untestable from other modules and means Phase 6 will have no shared constant to reference when writing audit rows — it will re-declare the list and diverge.
**Suggestion:** Extract `VOCAB` as `pub const AUDIT_KIND_VOCAB: &[&str]` in `core::audit` (or `core::storage::audit_vocab`) so Phase 6 can import it and the test can reference it by path.
**Claude's assessment:** Agree. This directly blocks Phase 6's audit-row writer from reusing the canonical vocabulary.

---

### F045 · [WARNING] AuditEvent::all_variants() and gen_mutation_schemas variant list are manually maintained
**Consensus:** SINGLE · flagged by: qwen
**File:** `crates/core/src/audit.rs` · **Lines:** 209-254
**Description:** `all_variants()` returns a hard-coded `vec![]` of 43 enum variants that must be manually kept in sync with the `AuditEvent` enum. `gen_mutation_schemas.rs` has the same manual list for `Mutation` variants. Adding a new variant silently excludes it from tests and schema generation with no compiler diagnostic.
**Suggestion:** Use `strum::EnumIter` or a proc-macro to derive the variant list from the enum definition, or add a compile-time `const` assertion that `all_variants().len() == <expected count>`.
**Claude's assessment:** Agree. The manual list is a maintenance trap. `strum` is the standard solution.

---

### F046 · [WARNING] serde_json Number precision trap applies to DesiredState JSON canonicalization
**Consensus:** SINGLE · flagged by: learnings_match (known-pattern match)
**File:** general (applies wherever DesiredState is serialized to canonical JSON)
**Description:** The project's solutions docs (`serde-json-number-variant-before-f64-cast-2026-05-05.md`) document a precision trap: when canonicalising `serde_json::Value` numbers, calling `n.as_f64()` without first checking `n.is_f64()` silently loses precision for `i64`/`u64` values larger than 2^53. Phase 4 introduces `DesiredState` JSON serialization (schema generation, diff values). If Phase 5 uses these serialized values for content-addressing or storage keys, precision loss would produce incorrect hashes.
**Suggestion:** Review any code that iterates `serde_json::Value::Number` in `DesiredState` serialization or canonicalization paths. Apply the `is_f64()` guard before `as_f64()` per the documented lesson.
**Claude's assessment:** Agree as a forward-looking guard. Check during Phase 5 implementation when canonicalization is wired.

---

### F047 · [WARNING] Optimistic concurrency TOCTOU gap — read-check-write needs BEGIN IMMEDIATE if SQLite-backed
**Consensus:** SINGLE · flagged by: learnings_match (known-pattern match)
**File:** general (applies to Phase 5 persistence adapter for DesiredState)
**Description:** Phase 4 introduces optimistic concurrency on `DesiredState` (version conflict check). The project's solutions docs (`sqlite-begin-immediate-read-check-write-2026-05-05.md`) document that any read-check-write sequence backed by SQLite must use `BEGIN IMMEDIATE` (not the default `DEFERRED`) to acquire the write lock before the invariant check. Using `DEFERRED` introduces a TOCTOU window where another writer can commit between the version read and the state write.
**Suggestion:** When the Phase 5 adapter implements `apply_mutation` persistence, use `BEGIN IMMEDIATE` for the transaction that reads the current version, runs the conflict check, and writes the updated state.
**Claude's assessment:** Agree. Flag this for the Phase 5 adapter implementer. The pure-core layer is fine; this is adapter-layer guidance.

---

### F048 · [WARNING] Schema-version DB column should be added at migration time, not defaulted at read time
**Consensus:** SINGLE · flagged by: learnings_match (known-pattern match)
**File:** general (applies to Phase 5 DesiredState persistence migration)
**Description:** The project's solutions docs (`schema-version-column-at-creation-2026-05-05.md`) document that when a Rust model field carries a schema-version marker, the DB column must be added in the same migration — retrofitting after a format bump is expensive. Phase 4 establishes `DesiredState` as the primary aggregate; if it carries a schema-encoding version, the column belongs in the Phase 5 initial migration.
**Suggestion:** When authoring the Phase 5 migration for `DesiredState` persistence, include a `schema_version` or `encoding_version` column from the start, even if its initial value is always 1.
**Claude's assessment:** Agree as a forward-looking note. Low effort to include upfront, expensive to retrofit.

---

### F049 · [WARNING] Open redirect — no URL scheme validation on RedirectRule.to
**Consensus:** SINGLE · flagged by: security
**File:** `crates/core/src/model/redirect.rs` · **Lines:** 8-12
**Description:** `RedirectRule.to` is a free-form `String` with no validation at the model or pre-condition layer. `CreateRoute`, `UpdateRoute`, and `ImportFromCaddyfile` pre-conditions do not inspect the `redirects` field. An attacker who can submit a mutation can set `to` to `javascript:`, `data:`, `//attacker.example`, or any protocol-relative URL, turning the proxy into an open redirector.
**Suggestion:** In `validate.rs`, add a `check_redirect_url_valid` function called from `CreateRoute` and `UpdateRoute` (and `ImportFromCaddyfile` once F001 is fixed) that parses the URL with `url::Url::parse` and rejects any scheme other than `http` or `https`.
**Claude's assessment:** Agree strongly. Open redirectors are OWASP Top 10 material. Even if the mutation API is internal-only today, the validation should exist at the core layer before any HTTP surface is wired.

---

### F050 · [WARNING] on_demand_ask_url accepted without URL format or SSRF validation
**Consensus:** SINGLE · flagged by: security
**File:** `crates/core/src/model/tls.rs` · **Lines:** 15-17
**Description:** `TlsConfig.on_demand_ask_url` is a free-form `Option<String>`. Caddy's on-demand TLS feature queries this URL before issuing a certificate for a hostname. There is no validation in `validate.rs` for `SetTlsConfig`. An operator or compromised client could set this to an internal service URL (SSRF), a non-HTTPS URL, or a URL that always returns 200 — effectively disabling the on-demand hostname allowlist and enabling certificate issuance for arbitrary domains.
**Suggestion:** In `pre_conditions` for `SetTlsConfig`: if `patch.on_demand_ask_url` is `Some(Some(_))`, parse the URL and reject any non-`https` scheme and any loopback/RFC 1918 destination (SSRF guard). Use `url::Url::parse` + IP parsing for the SSRF check.
**Claude's assessment:** Agree. This is a high-impact configuration field that can be weaponized if unvalidated. The SSRF risk is real once the apply layer is live.

---

### F051 · [WARNING] CidrMatcher wraps an unvalidated string — no CIDR syntax check
**Consensus:** MAJORITY · flagged by: security (WARNING), glm (SUGGESTION)
**File:** `crates/core/src/model/matcher.rs` · **Lines:** 64-82
**Description:** `CidrMatcher(pub String)` accepts any string without verifying it is valid CIDR notation. `pre_conditions` does not inspect `MatcherSet.remote` entries. An invalid CIDR (e.g. `"not-a-cidr"`, extra octets, out-of-range prefix) is accepted into `DesiredState` and only fails when Caddy tries to apply it, producing an opaque Caddy-level error rather than a clear rejection at mutation time.
**Suggestion:** Add a `check_matchers_valid` function in `validate.rs` called from `CreateRoute` and `UpdateRoute` pre-conditions that validates each `CidrMatcher` string using `std::net::Ipv4Addr`/`Ipv6Addr` + prefix-length parsing (or the `ipnet` crate).
**Claude's assessment:** Agree. Input validation belongs at the mutation boundary, not at the Caddy apply layer.

---

### F052 · [WARNING] RoutePatch/UpstreamPatch fields are .clone()d instead of moved — unnecessary allocation
**Consensus:** MAJORITY · flagged by: qwen (HIGH), glm (SUGGESTION)
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 357-377, 402-413
**Description:** Every patch field in `apply_route_patch` and `apply_upstream_patch` is applied via `.clone()` (e.g., `route_patch.hostnames.clone()`). Since the function already clones the entire state, and the patch is consumed after apply, this allocates twice for every patched `Vec` or `MatcherSet` field. For large configurations with many matchers or upstreams, this doubles memory pressure during every `UpdateRoute`.
**Suggestion:** Accept `route_patch: RoutePatch` (by value) and use `if let Some(v) = patch.hostnames { route.hostnames = v; }` to move values out. Or use `std::mem::take` on the `Option` fields.
**Claude's assessment:** Agree. Taking by value is idiomatic Rust and eliminates the double allocation. Not a correctness issue, but worth fixing before the apply layer is on the hot path.

---

### SUGGESTION / LOW Findings (this run)

### F053 · [SUGGESTION] check_detach_policy error-vs-no-op semantics are undocumented
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/src/mutation/validate.rs` · **Lines:** 185-197
**Description:** `check_detach_policy` returns `ValidationError { PolicyAttachmentMissing }` when the route has no policy attached. It is not documented whether this is intentional (detaching a non-attached policy is an error) or an oversight (it should be idempotent). Without a test for this case, the behavior could change unintentionally.
**Suggestion:** Add an explicit test for detaching when no attachment exists. If error is correct by design, add a doc comment to `check_detach_policy` stating that. If idempotency is intended, change the behavior and update the error.
**Claude's assessment:** Agree the behavior should be explicit. "Error on non-attached detach" is the safe default; document it.

---

### F054 · [SUGGESTION] apply_set_global_config clone_from semantics are non-obvious — needs a comment
**Consensus:** SINGLE · flagged by: minimax
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 253-280
**Description:** The pattern `new_state.global.field.clone_from(&None)` for the "clear" case (`Some(None)`) is subtle — `Option::clone_from` with a `None` source and `Some` target correctly replaces the target with `None`, but this is not immediately obvious to readers unfamiliar with `clone_from` semantics.
**Suggestion:** Add a one-line comment explaining that `clone_from(&None)` clears the field (three-state: `Some(None)` → clear, `Some(Some(v))` → set, `None` → no-op).
**Claude's assessment:** Agree. The CLAUDE.md comment policy allows a comment here — the behavior would surprise a reader. One line is sufficient.

---

### F055 · [SUGGESTION] Public enums lack #[non_exhaustive] — future variant additions are semver-breaking
**Consensus:** SINGLE · flagged by: qwen
**File:** `crates/core/src/mutation/types.rs` · **Lines:** 194-222
**Description:** `Mutation`, `MutationKind`, `ValidationRule`, `SchemaErrorKind`, and `ForbiddenReason` are all public enums without `#[non_exhaustive]`. Any downstream crate (adapters, CLI) that exhaustively matches these will break on every variant addition in future phases.
**Suggestion:** Add `#[non_exhaustive]` to all public enums defined in `crates/core`.
**Claude's assessment:** Partially agree. For enums that are meant to be exhaustively matched in `apply_variant` (e.g., `Mutation` itself), `#[non_exhaustive]` would force a `_ => unreachable!()` arm, which conflicts with the project's no-unreachable rule. Apply selectively: `ValidationRule`, `ForbiddenReason`, and `SchemaErrorKind` are good candidates; `Mutation` and `MutationKind` less so.

---

### F056 · [SUGGESTION] schemars and proptest dependencies not version-pinned — supply-chain risk
**Consensus:** SINGLE · flagged by: security
**File:** `crates/core/Cargo.toml` · **Lines:** general
**Description:** `schemars = "0.8"` and `proptest = "1"` use bare major-version constraints. Both are compile-time or test-time code execution surfaces: `schemars` macros run over all model types at build time; `proptest` runs at test/CI time. A supply-chain compromise in any `0.8.x` or `1.x.y` release would execute silently.
**Suggestion:** Pin to exact versions (`schemars = "=0.8.21"`, `proptest = "=1.6.0"` or current) in the workspace `[dependencies]` table so `cargo deny check` and the workspace lock govern them uniformly.
**Claude's assessment:** Agree in principle. Exact pinning combined with `cargo deny check` is the right combination. File a `cargo deny` advisory check as part of the gate.

---

### F057 · [SUGGESTION] Missing validation for black-hole routes (no upstream and no redirect rule)
**Consensus:** SINGLE · flagged by: gemini
**File:** `crates/core/src/mutation/validate.rs` · **Lines:** 14-20
**Description:** `CreateRoute` pre-conditions do not verify that a route has at least one upstream destination or a redirect rule. A route with neither is a "black hole" — it matches traffic and drops it with no configured handler, which will produce a Caddy configuration error at apply time.
**Suggestion:** Add a `check_route_has_destination` validation that rejects `CreateRoute` when both `upstreams` is empty and `redirects` is absent.
**Claude's assessment:** Agree. The validation is cheap and prevents a class of degenerate configurations from entering `DesiredState`.

---

### F058 · [SUGGESTION] Caddyfile import warnings are discarded — not surfaced in MutationOutcome
**Consensus:** SINGLE · flagged by: gemini
**File:** `crates/core/src/mutation/apply.rs` · **Lines:** 212-233
**Description:** `ParsedCaddyfile` includes a `warnings` field generated during parsing, but `apply_import_caddyfile` discards it. Users have no way to know their import was accepted with parser warnings.
**Suggestion:** Include import warnings in `MutationOutcome` or attach them to the resulting `AuditEvent`. At minimum, log them via `tracing::warn!`.
**Claude's assessment:** Agree. Discarding structured warnings from the parser is a UX gap. `MutationOutcome` is the right place.

---

### F059 · [SUGGESTION] HostPattern variant/content consistency not validated
**Consensus:** SINGLE · flagged by: gemini
**File:** `crates/core/src/mutation/validate.rs` · **Lines:** 108
**Description:** `check_hostnames_valid` calls `validate_hostname` to check if the string is valid, but does not verify that the provided `HostPattern` variant matches the content — e.g., `HostPattern::Exact("*.example.com")` passes validation even though `*.example.com` should produce `HostPattern::Wildcard`. This creates an inconsistency between the discriminant and the payload.
**Suggestion:** Assert that the provided variant matches the variant the factory function would produce for the same input.
**Claude's assessment:** Agree. The factory function should be the single source of truth for which variant a string maps to; bypassing it is a validation gap.

---

### F060 · [SUGGESTION] RedirectRule.status accepts any u16 — not constrained to valid HTTP redirect codes
**Consensus:** SINGLE · flagged by: glm
**File:** `crates/core/src/model/redirect.rs` · **Lines:** 9-10
**Description:** `RedirectRule.status` is `u16` with no validation. Values outside the valid HTTP redirect range (300-308) or non-standard redirect codes would be accepted without error and potentially generate an invalid Caddy configuration.
**Suggestion:** Add a validation function or pre-condition check constraining `status` to the set of valid HTTP redirect codes {300, 301, 302, 303, 307, 308}.
**Claude's assessment:** Agree. Closely related to F049 (open redirect URL validation) — fix in the same pass.

---

### F061 · [SUGGESTION] audit.rs should be audit/mod.rs per spec — future submodule extensibility
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `crates/core/src/audit.rs` · **Lines:** general
**Description:** The TODO (Slice 4.7) specifies `core/crates/core/src/audit/mod.rs` as the file path. The implementation creates `audit.rs` instead. Both compile identically but differ from the spec. If Phase 6 needs to add submodules (e.g., `audit/writer.rs`), a rename will be required at that point.
**Suggestion:** Rename `src/audit.rs` → `src/audit/mod.rs` now to match the spec and pre-empt the Phase 6 refactor.
**Claude's assessment:** Weak agree. The rename is mechanical and low-risk. If Phase 6 is imminent, do it now; otherwise it's low priority.

---

### F062 · [SUGGESTION] Extra ValidationRule variants diverge from phase spec without amendment note
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `crates/core/src/mutation/error.rs` · **Lines:** 59-76
**Description:** The TODO (Slice 4.7) lists exactly five `ValidationRule` variants. The implementation adds three more (`DuplicateUpstreamId`, `RouteMissing`, `UpstreamStillReferenced`) needed by Slice 4.9 pre-condition logic, but there is no recorded rationale or spec amendment.
**Suggestion:** Add a code comment on each extra variant referencing the slice that required it (e.g., `// added for Slice 4.9 DeleteUpstream referential integrity`) and update the phase TODO if it is used as a living spec.
**Claude's assessment:** Agree. The variants are correct additions; the spec just needs a note. Low effort.

---

### Newly Superseded in This Run

| Finding | Reviewer | Reason |
|---------|---------|--------|
| ImportFromCaddyfile bypass | code_adversarial, codex, gemini, glm, qwen | Same root as F001 (upgraded to UNANIMOUS) |
| ImportFromCaddyfile silent overwrite | codex, minimax | Same root as F004 (upgraded to UNANIMOUS) |
| Per-variant schema $ref invalid | code_adversarial, codex | Same as F015 |
| Property tests only CreateRoute | codex, glm | Same as F010 |
| RoutePatch doc comment inaccurate | glm, qwen | Same as F023 (consensus upgraded to MAJORITY) |
| Hardcoded relative path in build.rs | glm | Same as F008 |
| Diff/to_json silently discards errors | qwen, security | Same as F029 (consensus upgraded to MAJORITY · WARNING) |
| Identifier newtypes unvalidated | security | Same as F006 (consensus upgraded to MAJORITY · CRITICAL) |
| no-unreachable-in-production-match | learnings_match | Already fixed — cf425a4 |
| three-state-patch-double-option | learnings_match | Already fixed — cf425a4 |
| build.rs rerun-if-changed | scope_guardian | Same as F012 |
| apply_route_patch redundant clone | glm SUGGESTION | Same as F052 (this run) |

---

### Re-run Summary Statistics

| Severity | Unanimous | Majority | Single | Total (this run) |
|----------|-----------|----------|--------|-----------------|
| CRITICAL | 0 | 0 | 1 | 1 |
| HIGH | 0 | 1 | 3 | 4 |
| WARNING | 0 | 5 | 13 | 18 |
| SUGGESTION | 0 | 0 | 10 | 10 |
| **Total** | **0** | **6** | **27** | **33** |

**Cumulative actionable (F001–F062):** 62 findings
**Total superseded across both runs:** 46

# Phase 4 — Fixed Findings

**Run date:** 2026-05-06
**Total fixed:** 50

| ID | Severity | Title | File | Commit | PR | Date |
|----|----------|-------|------|--------|----|------|
| F001 | HIGH | ImportFromCaddyfile bypasses all pre-condition validators | `crates/core/src/mutation/apply.rs` | `7d90fc1` | — | 2026-05-06 |
| F004 | WARNING | ImportFromCaddyfile emits coarse whole-map diffs and silently overwrites entities | `crates/core/src/mutation/apply.rs` | `87e022a` | — | 2026-05-06 |
| F005 | WARNING | apply_set_global_config and apply_set_tls_config emit no-op diffs | `crates/core/src/mutation/apply.rs` | `87e022a` | — | 2026-05-06 |
| F006 | CRITICAL | Identifier newtypes accept any string — no ULID validation | `crates/core/src/model/identifiers.rs` | `d826850` | — | 2026-05-06 |
| F007 | WARNING | schemars lives in core dependencies rather than gated as feature dep | `crates/core/Cargo.toml` | `87e022a` | — | 2026-05-06 |
| F008 | WARNING | Hardcoded relative path in build.rs | `crates/core/build.rs` | `87e022a` | — | 2026-05-06 |
| F009 | WARNING | gen_mutation_schemas uses process::exit(1) — bypasses drop semantics | `crates/core/src/bin/gen_mutation_schemas.rs` | `87e022a` | — | 2026-05-06 |
| F010 | WARNING | Proptest suite only covers CreateRoute — missing 12 other mutation variants | `crates/core/tests/mutation_props.rs` | `21e330d` | — | 2026-05-06 |
| F011 | WARNING | All-None GlobalConfigPatch accepted as silent no-op | `crates/core/src/mutation/validate.rs` | `87e022a` | — | 2026-05-06 |
| F012 | WARNING | build.rs missing cargo:rerun-if-changed for schema output directory | `crates/core/build.rs` | `87e022a` | — | 2026-05-06 |
| F013 | WARNING | ForbiddenReason does not implement Display — error messages show Debug output | `crates/core/src/mutation/error.rs` | `21e330d` | — | 2026-05-06 |
| F014 | WARNING | Per-variant schema stub files lack $id field | `crates/core/src/bin/gen_mutation_schemas.rs` | `87e022a` | — | 2026-05-06 |
| F015 | WARNING | Per-variant schema stub $ref points to a non-existent definition | `crates/core/src/bin/gen_mutation_schemas.rs` | `87e022a` | — | 2026-05-06 |
| F016 | SUGGESTION | check_upstreams_exist error path omits the offending upstream index | `crates/core/src/mutation/validate.rs` | `7f465f0` | — | 2026-05-06 |
| F017 | SUGGESTION | Misleading error for empty hostname labels — EmptyLabel variant missing | `crates/core/src/model/route.rs` | `7f465f0` | — | 2026-05-06 |
| F018 | SUGGESTION | Test idempotency_on_mutation_id verifies determinism not idempotency — rename | `crates/core/tests/mutation_props.rs` | `7f465f0` | — | 2026-05-06 |
| F020 | SUGGESTION | allow(clippy::option_option) is struct-level not field-level | `crates/core/src/mutation/patches.rs` | `7f465f0` | — | 2026-05-06 |
| F023 | SUGGESTION | RoutePatch doc comment inaccurately describes triple-state semantics | `crates/core/src/mutation/patches.rs` | `21e330d` | — | 2026-05-06 |
| F024 | SUGGESTION | Schema stubs use non-standard x-variant extension field | `crates/core/src/bin/gen_mutation_schemas.rs` | `7f465f0` | — | 2026-05-06 |
| F027 | SUGGESTION | Dead audit_event_for arm for Rollback should be annotated | `crates/core/src/mutation/apply.rs` | `7f465f0` | — | 2026-05-06 |
| F028 | SUGGESTION | MutationOutcome.kind field should be renamed audit_event | `crates/core/src/mutation/outcome.rs` | `7f465f0` | — | 2026-05-06 |
| F029 | WARNING | Diff serialization uses .ok() — errors silently produce null diffs | `crates/core/src/mutation/apply.rs` | `21e330d` | — | 2026-05-06 |
| F030 | CRITICAL | schema_drift integration test missing — CI cannot detect schema drift | `crates/core/tests/schema_drift.rs` | `d826850` | — | 2026-05-06 |
| F031 | HIGH | Audit diff before-state reads from mutated state, not original | `crates/core/src/mutation/apply.rs` | `7d90fc1` | — | 2026-05-06 |
| F032 | HIGH | apply_upgrade_policy silently no-ops when policy_attachment is None | `crates/core/src/mutation/apply.rs` | `7d90fc1` | — | 2026-05-06 |
| F033 | HIGH | Route.updated_at is never updated — responsibility undocumented | `crates/core/src/mutation/apply.rs` | `7d90fc1` | — | 2026-05-06 |
| F034 | HIGH | content_address SHA-256 utility out of scope in mutation/types.rs | `crates/core/src/canonical_json.rs` | `de47342` | — | 2026-05-06 |
| F035 | WARNING | DesiredState.version is public — callers can bypass +1 invariant | `crates/core/src/mutation/apply.rs` | `3787298` | — | 2026-05-06 |
| F036 | WARNING | Version increment unchecked — i64::MAX overflow causes panic | `crates/core/src/mutation/apply.rs` | `21e330d` | — | 2026-05-06 |
| F037 | WARNING | AttachPolicy accepts preset_version == 0 | `crates/core/src/mutation/validate.rs` | `21e330d` | — | 2026-05-06 |
| F038 | WARNING | justfile missing check-schemas recipe | `Justfile` | `21e330d` | — | 2026-05-06 |
| F039 | WARNING | Non-object mutation payload produces MissingExpectedVersion instead of Malformed | `crates/core/src/mutation/envelope.rs` | `21e330d` | — | 2026-05-06 |
| F040 | WARNING | SetTlsConfig capability check only gates on ACME email, not all TLS fields | `crates/core/src/mutation/capability.rs` | `21e330d` | — | 2026-05-06 |
| F041 | WARNING | CreateRoute capability uses wrong module name for header handler | `crates/core/src/mutation/capability.rs` | `21e330d` | — | 2026-05-06 |
| F042 | WARNING | AttachPolicy/UpgradePolicy version mismatch uses generic PolicyPresetMissing error | `crates/core/src/mutation/validate.rs` | `21e330d` | — | 2026-05-06 |
| F043 | WARNING | check_detach_policy and check_upgrade_policy perform redundant map lookups | `crates/core/src/mutation/validate.rs` | `21e330d` | — | 2026-05-06 |
| F044 | WARNING | AUDIT_KIND_VOCAB only accessible in test scope | `crates/core/src/audit.rs` | `21e330d` | — | 2026-05-06 |
| F045 | WARNING | No compile-time guard against adding AuditEvent variants without updating VOCAB | `crates/core/src/audit.rs` | `21e330d` | — | 2026-05-06 |
| F049 | WARNING | No validation of redirect URL scheme — open redirect to arbitrary protocols | `crates/core/src/mutation/validate.rs` | `21e330d` | — | 2026-05-06 |
| F050 | WARNING | on_demand_ask_url accepts loopback and RFC 1918 addresses — SSRF vector | `crates/core/src/mutation/validate.rs` | `21e330d` | — | 2026-05-06 |
| F051 | WARNING | CIDR matchers accept invalid notation — no parse-time validation | `crates/core/src/mutation/validate.rs` | `21e330d` | — | 2026-05-06 |
| F052 | WARNING | RoutePatch/UpstreamPatch fields cloned instead of moved — unnecessary allocation | `crates/core/src/mutation/apply.rs` | `3787298` | — | 2026-05-06 |
| F053 | SUGGESTION | check_detach_policy error-vs-no-op semantics undocumented | `crates/core/src/mutation/validate.rs` | `6e70eca` | — | 2026-05-06 |
| F054 | SUGGESTION | apply_set_global_config clone_from semantics non-obvious — needs comment | `crates/core/src/mutation/apply.rs` | `6e70eca` | — | 2026-05-06 |
| F055 | SUGGESTION | Public enums lack #[non_exhaustive] — future variants are semver-breaking | `crates/core/src/mutation/error.rs` | `3971998` | — | 2026-05-06 |
| F056 | SUGGESTION | schemars and proptest dependencies not version-pinned — supply-chain risk | `crates/core/Cargo.toml` | `3971998` | — | 2026-05-06 |
| F057 | SUGGESTION | Missing validation for black-hole routes (no upstream and no redirect) | `crates/core/src/mutation/validate.rs` | `3787298` | — | 2026-05-06 |
| F059 | SUGGESTION | HostPattern variant/content consistency not validated | `crates/core/src/mutation/validate.rs` | `6e70eca` | — | 2026-05-06 |
| F060 | SUGGESTION | RedirectRule.status accepts any u16 — not constrained to valid redirect codes | `crates/core/src/mutation/validate.rs` | `3787298` | — | 2026-05-06 |
| F062 | SUGGESTION | Extra ValidationRule variants diverge from phase spec without amendment note | `crates/core/src/mutation/error.rs` | `6e70eca` | — | 2026-05-06 |

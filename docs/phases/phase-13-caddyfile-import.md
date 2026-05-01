# Phase 13 — Caddyfile import

Source of truth: [`../phases/phased-plan.md#phase-13--caddyfile-import`](../phases/phased-plan.md#phase-13--caddyfile-import).

## Pre-flight checklist

- [ ] Phase 12 complete: snapshot writer, audit log, rollback, desired-state aggregate are integration-tested.
- [ ] The mutation algebra exposes `CreateRoute`, `UpdateRoute`, `AttachPolicy`, `SetUpstream`, `SetTls`, `SetHeaders`, `SetMatchers`, `SetRedirect`, `SetBodyLimit`, `SetEncoding`.
- [ ] Caddy binary available on the test runner so the round-trip harness can call `caddy adapt` and `caddy run`. The minimum supported Caddy version is **2.8.0**; the continuous integration test target is pinned to **2.11.2** (the latest stable release as of 2026-04-30, the date this phase was authored). The Caddyfile grammar reference version is also 2.11.2.
- [ ] **Caddy version pin file present.** Create `caddy-version.txt` at the repository root containing the literal string `2.11.2` on a single line, terminated by `\n`. The CI workflow and the integration-test bootstrap MUST read this file and install exactly that Caddy version. The pin MUST be reviewed every Caddy minor release; bumping is a single-commit change to `caddy-version.txt` plus golden regeneration. (`core/Cargo.toml` test dev-dependencies is the alternative location; the free-standing file is preferred because the pin is consumed by shell scripts and Docker layers, not just `cargo`.)
- [ ] `pretty_assertions`, `insta`, and `proptest` are dev-dependencies in `core/crates/core/Cargo.toml`.

## Tasks

### Lexer (core)

- [ ] **Define the `Token` and `Span` types.**
  - Module: `core/crates/core/src/caddyfile/lexer.rs`.
  - Acceptance: `pub enum Token` MUST contain variants `Word(String)`, `String(String)`, `OpenBrace`, `CloseBrace`, `Newline`, `Comment(String)`, `EnvSubstitution { name: String, default: Option<String> }`, `Placeholder { path: String }`, `LineContinuation`, `EndOfFile`. `pub struct Span { line: u32, column: u32, byte_offset: u32, byte_length: u32 }` MUST be `Copy + Clone + Debug + PartialEq`.
  - Done when: `cargo test -p trilithon-core caddyfile::lexer::tests::token_types` enumerates every variant and asserts trait derivations.
  - Feature: T1.5.
- [ ] **Implement `pub fn lex(input: &str) -> Result<Vec<Token>, LexError>`.**
  - Module: `core/crates/core/src/caddyfile/lexer.rs`.
  - Acceptance: The lexer MUST handle backslash continuations, double-quoted and backtick-quoted strings (with escape sequences `\\`, `\n`, `\t`, `\"`), inline `#` comments, environment substitution `{$VAR}` and `{$VAR:default}`, and Caddy placeholders `{http.request.host}` (treated as opaque placeholders). It MUST emit a `LexError` with line and column on unterminated strings, unbalanced braces, and invalid UTF-8.
  - Done when: `cargo test -p trilithon-core caddyfile::lexer::tests` passes including a fuzz test using `proptest` for arbitrary UTF-8 inputs (no panic).
  - Feature: T1.5.
- [ ] **Lexer fuzz harness.**
  - Module: `core/crates/core/fuzz/fuzz_targets/caddyfile_lexer.rs`.
  - Acceptance: The fuzz target MUST run for at least 60 seconds in CI without producing a crash or timeout.
  - Done when: `cargo +nightly fuzz run caddyfile_lexer -- -max_total_time=60` returns zero in CI.
  - Feature: T1.5 (mitigates H15).

### Parser (core)

- [ ] **Define the `CaddyfileAst` node types.**
  - Module: `core/crates/core/src/caddyfile/ast.rs`.
  - Acceptance: `pub struct SiteBlock { addresses: Vec<Address>, body: Vec<Directive>, span: Span }`, `pub struct Directive { name: String, args: Vec<Argument>, body: Option<Vec<Directive>>, span: Span }`, `pub struct MatcherDefinition { name: String, body: Vec<MatcherClause>, span: Span }`, `pub struct Snippet { name: String, body: Vec<Directive>, span: Span }`, `pub enum ImportTarget { File(PathBuf), Snippet(String) }`, `pub struct CaddyfileAst { globals: Vec<Directive>, snippets: HashMap<String, Snippet>, sites: Vec<SiteBlock> }` MUST be defined with `serde::Serialize` for golden-file dumping.
  - Done when: `cargo build -p trilithon-core` succeeds and a unit test serialises a fixture AST to JSON.
  - Feature: T1.5.
- [ ] **Implement `pub fn parse(tokens: &[Token], opts: &ParseOptions) -> Result<CaddyfileAst, ParseError>`.**
  - Module: `core/crates/core/src/caddyfile/parser.rs`.
  - Acceptance: The parser MUST be recursive-descent, MUST track nesting depth against `opts.max_nesting_depth`, and MUST emit `ParseError::Structural { line, column, message }` with line/column for syntax errors.
  - Done when: `cargo test -p trilithon-core caddyfile::parser::tests` passes including the structural error tests.
  - Feature: T1.5.
- [ ] **Snippet expander with cycle detection.**
  - Module: `core/crates/core/src/caddyfile/expand.rs`.
  - Acceptance: `pub fn expand(ast: CaddyfileAst, opts: &ExpandOptions) -> Result<CaddyfileAst, ExpandError>` MUST inline `import <snippet>` directives, MUST detect cycles (`a imports b imports a`) and reject with `ExpandError::ImportCycle { chain }`, and MUST track the expansion factor against `opts.max_snippet_expansion`.
  - Done when: `cargo test -p trilithon-core caddyfile::expand::tests::cycle_detected` and `expansion_factor_bound` pass.
  - Feature: T1.5.
- [ ] **Environment substitution resolver.**
  - Module: `core/crates/core/src/caddyfile/env.rs`.
  - Acceptance: `pub fn resolve_env(ast: &mut CaddyfileAst, env: &dyn EnvProvider) -> Vec<LossyWarning>` MUST replace every `EnvSubstitution` token, MUST emit `LossyWarning::EnvSubstitutionEmpty` for unresolved variables without a default, MUST be deterministic.
  - Done when: a unit test asserts the warning is emitted for an unset variable without a default and absent for one with a default.
  - Feature: T1.5.
- [ ] **File-import resolver.**
  - Module: `core/crates/core/src/caddyfile/import.rs`.
  - Acceptance: `pub fn resolve_file_imports(ast: CaddyfileAst, root: &Path, opts: &ImportOptions) -> Result<CaddyfileAst, ImportError>` MUST resolve relative paths against `root`, MUST refuse paths outside `root` (path traversal), MUST detect file-import cycles.
  - Done when: a unit test exercises traversal rejection (`../etc/passwd`) and cycle detection.
  - Feature: T1.5 (mitigates H15).

### Translator (core)

- [ ] **Define `LossyWarning`, `TranslateContext`, `TranslateResult`.**
  - Module: `core/crates/core/src/caddyfile/lossy.rs` and `core/crates/core/src/caddyfile/translator.rs`.
  - Acceptance: `LossyWarning` MUST contain the variants enumerated in the phased plan (`UnsupportedDirective`, `CommentLoss`, `OrderingLoss`, `SnippetExpansionLoss`, `EnvSubstitutionEmpty`, `TlsDnsProviderUnavailable`, `PlaceholderPassthrough`, `CapabilityDegraded`). Each variant MUST have a stable kebab-case identifier accessible via `pub fn id(&self) -> &'static str`.
  - Done when: a unit test enumerates every variant and asserts `id` is unique.
  - Feature: T1.5.
- [ ] **Translator: site-address blocks → routes.**
  - Module: `core/crates/core/src/caddyfile/translator/sites.rs`.
  - Acceptance: For each `SiteBlock`, the translator MUST produce a `CreateRoute` mutation per resolved address with hostname, port, and a stable route id (ULID).
  - Done when: a unit test against `01_trivial/single_host` produces the expected `CreateRoute` mutations.
  - Feature: T1.5.
- [ ] **Translator: `reverse_proxy` → `SetUpstream`.**
  - Module: `core/crates/core/src/caddyfile/translator/reverse_proxy.rs`.
  - Acceptance: The translator MUST handle `to <upstream>...`, `lb_policy`, `health_uri`, `header_up`, `header_down`, and `transport http { ... }` (TLS, H2C). Any unknown sub-directive MUST emit `UnsupportedDirective`.
  - Done when: `02_reverse_proxy/*` fixtures translate to the expected mutations.
  - Feature: T1.5.
- [ ] **Translator: `file_server` → desired-state file-server route.**
  - Module: `core/crates/core/src/caddyfile/translator/file_server.rs`.
  - Acceptance: The translator MUST record `root`, `browse`, `hide`, and `index` and MUST produce a `SetFileServer` mutation.
  - Done when: a fixture-driven unit test passes.
  - Feature: T1.5.
- [ ] **Translator: `redir` → `SetRedirect`.**
  - Module: `core/crates/core/src/caddyfile/translator/redir.rs`.
  - Acceptance: The translator MUST translate `redir <target> <status>` and MUST default the status to 302 when omitted.
  - Done when: a fixture-driven unit test passes.
  - Feature: T1.5.
- [ ] **Translator: `route`, `handle`, `handle_path` → ordered handler list.**
  - Module: `core/crates/core/src/caddyfile/translator/handlers.rs`.
  - Acceptance: The translator MUST preserve ordering semantics, attach matchers correctly, and emit `OrderingLoss` if encountered ambiguity (a `route` containing nested `route`s with non-orthogonal matchers).
  - Done when: a fixture-driven unit test passes.
  - Feature: T1.5.
- [ ] **Translator: `header` → `SetHeaders`.**
  - Module: `core/crates/core/src/caddyfile/translator/headers.rs`.
  - Acceptance: The translator MUST handle `set`, `add`, `delete`, and replacement (`>`) on both request and response headers.
  - Done when: a fixture-driven unit test passes.
  - Feature: T1.5.
- [ ] **Translator: `respond` → `SetStaticResponse`.**
  - Module: `core/crates/core/src/caddyfile/translator/respond.rs`.
  - Acceptance: The translator MUST set status, body, and `close` flag.
  - Done when: a fixture-driven unit test passes.
  - Feature: T1.5.
- [ ] **Translator: `tls` → `SetTls`.**
  - Module: `core/crates/core/src/caddyfile/translator/tls.rs`.
  - Acceptance: The translator MUST handle `internal`, email-issuer, explicit cert/key file paths, and `dns <provider>`. The DNS-provider variant MUST emit `TlsDnsProviderUnavailable` when the capability cache reports the provider module is missing.
  - Done when: `09_tls/*` fixtures translate; the DNS-provider negative test passes.
  - Feature: T1.5 (coordinates with H5).
- [ ] **Translator: `encode` → `SetEncoding`.**
  - Module: `core/crates/core/src/caddyfile/translator/encode.rs`.
  - Acceptance: The translator MUST translate `gzip` and `zstd`. Any other algorithm emits `UnsupportedDirective`.
  - Done when: a fixture-driven unit test passes.
  - Feature: T1.5.
- [ ] **Translator: `log` → desired-state log directive.**
  - Module: `core/crates/core/src/caddyfile/translator/log.rs`.
  - Acceptance: The translator MUST translate `output`, `format`, `include`, `exclude`.
  - Done when: a fixture-driven unit test passes.
  - Feature: T1.5.
- [ ] **Translator: matchers (`@name { ... }`).**
  - Module: `core/crates/core/src/caddyfile/translator/matchers.rs`.
  - Acceptance: The translator MUST translate `path`, `path_regexp`, `host`, `header`, `method`, `query`, `expression`, `not`, `protocol`, `remote_ip`, `client_ip` named matchers.
  - Done when: `04_path_matchers/*` and `05_regex_matchers/*` fixtures translate.
  - Feature: T1.5.
- [ ] **Translator: catch-all unsupported-directive emitter.**
  - Module: `core/crates/core/src/caddyfile/translator/dispatch.rs`.
  - Acceptance: Any directive not handled by a specific translator MUST emit `LossyWarning::UnsupportedDirective` with the directive name and source location and MUST NOT abort translation.
  - Done when: a unit test asserts a synthetic `php_fastcgi` directive produces the warning and translation completes.
  - Feature: T1.5.

### Size guards

- [ ] **Input-size guard before lexer.**
  - Module: `core/crates/core/src/caddyfile/limits.rs`.
  - Acceptance: `pub fn check_input_size(bytes: &[u8], opts: &SizeOptions) -> Result<(), ImportError>` MUST reject inputs over 5 MiB by default before lexing.
  - Done when: an integration test with a 6 MiB blob asserts rejection within 100 ms.
  - Feature: T1.5 (mitigates H15).
- [ ] **Directive-count guard during parsing.**
  - Acceptance: The parser MUST track running directive count and abort with `ImportError::SizeExceeded { kind: "directives", observed, allowed }` past 10,000 by default.
  - Done when: an integration test with a synthetic 12,000-directive fixture asserts rejection within two seconds.
  - Feature: T1.5 (mitigates H15).
- [ ] **Nesting-depth guard.**
  - Acceptance: The parser MUST abort with `ImportError::SizeExceeded { kind: "nesting", observed, allowed }` past 32 levels by default.
  - Done when: a fixture with 33 nested matchers asserts rejection.
  - Feature: T1.5 (mitigates H15).
- [ ] **Snippet-expansion-factor guard.**
  - Acceptance: The expander MUST track `expanded_bytes / source_bytes` and abort past 100× by default.
  - Done when: a unit test asserts rejection on a snippet that fans out 200×.
  - Feature: T1.5 (mitigates H15).
- [ ] **Route-count guard after translation.**
  - Acceptance: The translator MUST count produced `CreateRoute` mutations and abort past 5,000 by default.
  - Done when: a fixture with 6,000 sites asserts rejection.
  - Feature: T1.5 (mitigates H15).

### Mutation wiring

- [ ] **`ImportFromCaddyfile` mutation type.**
  - Module: `core/crates/core/src/mutation.rs`.
  - Acceptance: `pub struct ImportFromCaddyfile { source_bytes: Vec<u8>, source_name: Option<String>, expected_version: i64 }` MUST be added to the `TypedMutation` enum and MUST persist `source_bytes` and the `LossyWarningSet` in the resulting snapshot's `metadata` blob.
  - Done when: a unit test asserts the mutation's `apply` produces a snapshot with the expected `intent` (`"Imported from Caddyfile: <source_name>"`) and metadata.
  - Feature: T1.5.
- [ ] **HTTP endpoints for preview and apply.**
  - Module: `core/crates/cli/src/http/imports.rs`.
  - Acceptance: `POST /api/v1/imports/caddyfile/preview` returns `{ mutations, warnings }`. `POST /api/v1/imports/caddyfile/apply` returns `{ snapshot_id, warnings }` and writes the audit row.
  - Done when: integration tests cover both endpoints.
  - Feature: T1.5.
- [ ] **Audit row authoring.**
  - Module: `core/crates/core/src/audit.rs`.
  - Acceptance: Add `AuditKind::ImportCaddyfile` and authoring code-path producing the row shape `{ kind: "import.caddyfile", target_kind: "snapshot", target_id, notes: { source_name, source_bytes_len, warning_count, warning_kinds } }`.
  - Done when: an integration test asserts the audit row on import.
  - Feature: T1.5 / T1.7.

### Fixture corpus

- [ ] **Author batch `01_trivial` fixtures.**
  - Path: `core/crates/core/tests/fixtures/caddyfile/01_trivial/`.
  - Acceptance: Three fixtures (single-host, single-host-custom-port, single-bare-address-global) MUST exist with `caddyfile`, `mutations.golden.json`, `warnings.golden.json`, `caddy-adapt.golden.json`, and `requests.ndjson`.
  - Done when: `cargo test -p trilithon-core caddyfile::corpus::trivial` passes.
  - Feature: T1.5.
- [ ] **Author batch `02_reverse_proxy` fixtures.**
  - Path: `.../02_reverse_proxy/`. Four fixtures (single, multi, healthcheck, h2c-transport).
  - Done when: corpus tests pass for the batch.
  - Feature: T1.5.
- [ ] **Author batch `03_virtual_hosts` fixtures.**
  - Path: `.../03_virtual_hosts/`. Three fixtures (two-hosts, three-hosts-distinct-tls, host-port-block).
  - Done when: corpus tests pass for the batch.
  - Feature: T1.5.
- [ ] **Author batch `04_path_matchers` fixtures.**
  - Path: `.../04_path_matchers/`. Three fixtures (`handle_path /api/*`, named `@api`, `path_regexp` with capture).
  - Done when: corpus tests pass for the batch.
  - Feature: T1.5.
- [ ] **Author batch `05_regex_matchers` fixtures.**
  - Path: `.../05_regex_matchers/`. Two fixtures (`path_regexp` with backreference, `not` with `path_regexp`).
  - Done when: corpus tests pass for the batch.
  - Feature: T1.5.
- [ ] **Author batch `06_snippets` fixtures.**
  - Path: `.../06_snippets/`. Three fixtures (single-snippet, multi-site-snippet, snippet-imports-snippet).
  - Done when: corpus tests pass for the batch.
  - Feature: T1.5.
- [ ] **Author batch `07_imports` fixtures.**
  - Path: `.../07_imports/`. Two fixtures (relative-path-file, env-within-import).
  - Done when: corpus tests pass for the batch.
  - Feature: T1.5.
- [ ] **Author batch `08_env_substitution` fixtures.**
  - Path: `.../08_env_substitution/`. Two fixtures (resolves, fallback-and-empty).
  - Done when: corpus tests pass; the empty-substitution fixture asserts a `LossyWarning::EnvSubstitutionEmpty`.
  - Feature: T1.5.
- [ ] **Author batch `09_tls` fixtures.**
  - Path: `.../09_tls/`. Three fixtures (`tls internal`, email issuer, explicit cert/key).
  - Done when: corpus tests pass.
  - Feature: T1.5.
- [ ] **Author batch `10_multi_site_one_file` fixtures.**
  - Path: `.../10_multi_site_one_file/`. Two fixtures (10 sites, 20 sites with shared snippet).
  - Done when: corpus tests pass.
  - Feature: T1.5.
- [ ] **Author batch `11_pathological` fixtures.**
  - Path: `.../11_pathological/`. Three fixtures (32-level-nesting, 15000-sites, 8MiB-line). All MUST be rejected by the size guards.
  - Done when: an integration test asserts rejection within two seconds and resident memory under 256 MiB.
  - Feature: T1.5 (mitigates H15).

### Round-trip equivalence harness

- [ ] **Implement the equivalence harness.**
  - Module: `core/crates/adapters/tests/caddyfile_round_trip.rs`.
  - Acceptance: For each non-pathological fixture, the harness MUST execute the seven-step methodology in the phased plan, including the live `caddy run` request matrix replay.
  - Done when: `cargo test -p trilithon-adapters caddyfile_round_trip` passes for every non-pathological fixture.
  - Feature: T1.5.
- [ ] **Implement and unit-test the normalisation rules.**
  - Module: `core/crates/core/src/caddyfile/normalise.rs`.
  - Acceptance: Each named transformation (`sort_object_keys`, `strip_trilithon_id_annotations`, `fold_equivalent_matcher_arrays`, `align_automatic_https_disable_redirects`) MUST be a `pub fn` with its own unit test.
  - Done when: each transformation has a passing unit test.
  - Feature: T1.5.

### Web UI

- [ ] **Author the Import wizard component.**
  - Path: `web/src/features/caddyfile-import/ImportWizard.tsx`.
  - Acceptance: Three steps (paste-or-upload, preview, confirm-and-import). MUST call `POST /api/v1/imports/caddyfile/preview` and `POST /api/v1/imports/caddyfile/apply`. Signature: `export function ImportWizard(): JSX.Element`.
  - Done when: a Vitest component test exercises all three steps with a stubbed adapter.
  - Feature: T1.5.
- [ ] **Author the reusable `LossyWarningList` component.**
  - Path: `web/src/components/LossyWarningList.tsx`.
  - Acceptance: Signature `export function LossyWarningList(props: { warnings: readonly LossyWarning[] }): JSX.Element`. MUST render each warning's stable identifier, message, and source location and MUST be reused by Phase 25's Caddyfile export panel.
  - Done when: a Vitest test renders a fixture warning list with axe-checked accessibility.
  - Feature: T1.5.
- [ ] **Author the `MutationPreviewList` component.**
  - Path: `web/src/features/caddyfile-import/MutationPreviewList.tsx`.
  - Acceptance: Renders the typed mutations the translator produced.
  - Done when: a Vitest test renders the list against a fixture preview.
  - Feature: T1.5.

### Cross-cutting tests

- [ ] **Pathological-input rejection within two seconds.**
  - Acceptance: An integration test MUST exercise each pathological fixture and assert rejection within two seconds and resident memory under 256 MiB on reference hardware.
  - Done when: the test passes in CI.
  - Feature: T1.5 (mitigates H15).
- [ ] **Lossy-warning catalogue completeness test.**
  - Acceptance: A test MUST iterate every `LossyWarning` variant and assert at least one fixture in the corpus produces it.
  - Done when: the test passes.
  - Feature: T1.5.

## Cross-references

- ADR-0001 (Caddy as the only supported reverse proxy).
- ADR-0002 (Caddy JSON Admin API as source of truth — Caddyfile is one-way).
- PRD T1.5 (Caddyfile one-way import).
- Architecture: "Caddyfile import," "Lossy warnings," "Failure modes — pathological imports."
- Hazards: H15 (Configuration import that hangs the proxy).

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] For every non-pathological fixture, the round-trip equivalence harness reports byte-identical normalised JSON and matching request-matrix responses against `caddy adapt`.
- [ ] Every parse that loses information emits at least one `LossyWarning` of the appropriate variant; the corpus covers every variant.
- [ ] All five size bounds reject pathological fixtures within two seconds on reference hardware without resident memory exceeding 256 MiB.
- [ ] The `ImportFromCaddyfile` mutation attaches the original bytes and the warning set to the resulting snapshot.
- [ ] Audit row authoring is exercised by an integration test.

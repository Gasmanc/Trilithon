# Phase 13 — Caddyfile import — Implementation Slices

> Phase reference: [../phases/phase-13-caddyfile-import.md](../phases/phase-13-caddyfile-import.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [phase-13-caddyfile-import.md](../phases/phase-13-caddyfile-import.md).
- Architecture sections: §4.1 (`core` purity rules), §4.2 (`adapters` boundary), §6.5 (`snapshots` row shape), §6.6 (`audit_log` and the V1 `kind` vocabulary, especially `import.caddyfile`), §6.7 (`mutations`), §10 (failure model rows for SQLite locked, validation rejection), §12.1 (tracing vocabulary), §13 (memory ceiling 256 MiB).
- Trait signatures: §1 `core::storage::Storage`, §6 `core::reconciler::Applier` (only the `validate` method is used here), §9 `core::config::EnvProvider`.
- ADRs: ADR-0001 (Caddy as the only supported reverse proxy), ADR-0002 (Caddy JSON Admin API as source of truth, Caddyfile is one-way), ADR-0009 (immutable content-addressed snapshots).
- PRD T1.5 (Caddyfile one-way import), T1.6 (typed mutation API), T1.7 (audit log).
- Hazards: H15 (configuration import that hangs the proxy).
- Caddy version pin: `caddy-version.txt` at the repo root contains `2.11.2\n`. The CI bootstrap installs exactly that version.

## Slice plan summary

| # | Slice title | Primary files | Effort (h) | Depends on |
|---|---|---|---|---|
| 13.1 | Lexer: token types and `lex` function | `core/crates/core/src/caddyfile/lexer.rs` | 8 | Phase 12 |
| 13.2 | Lexer fuzz harness | `core/crates/core/fuzz/fuzz_targets/caddyfile_lexer.rs` | 3 | 13.1 |
| 13.3 | Parser AST types and recursive-descent `parse` | `core/crates/core/src/caddyfile/ast.rs`, `parser.rs` | 10 | 13.1 |
| 13.4 | Snippet expander, env resolver, file-import resolver | `core/crates/core/src/caddyfile/expand.rs`, `env.rs`, `import.rs` | 8 | 13.3 |
| 13.5 | Size guards (input, directives, nesting, expansion factor, route count) | `core/crates/core/src/caddyfile/limits.rs` | 5 | 13.3, 13.4 |
| 13.6 | `LossyWarning` catalogue and translator dispatch skeleton | `core/crates/core/src/caddyfile/lossy.rs`, `translator/mod.rs`, `translator/dispatch.rs` | 6 | 13.3 |
| 13.7 | Translator batch A: sites, `reverse_proxy`, `file_server`, `redir`, `respond` | `core/crates/core/src/caddyfile/translator/{sites,reverse_proxy,file_server,redir,respond}.rs` | 12 | 13.6 |
| 13.8 | Translator batch B: handlers (`route`/`handle`/`handle_path`), `header`, `tls`, `encode`, `log`, matchers | `core/crates/core/src/caddyfile/translator/{handlers,headers,tls,encode,log,matchers}.rs` | 14 | 13.6 |
| 13.9 | Fixture corpus authoring batches `01_trivial` through `06_snippets` | `core/crates/core/tests/fixtures/caddyfile/01_trivial..06_snippets/` and `core/crates/core/tests/caddyfile_corpus.rs` | 12 | 13.7, 13.8 |
| 13.10 | Fixture corpus authoring batches `07_imports` through `11_pathological` | `core/crates/core/tests/fixtures/caddyfile/07_imports..11_pathological/` | 10 | 13.5, 13.9 |
| 13.11 | Round-trip equivalence harness and normalisation rules | `core/crates/adapters/tests/caddyfile_round_trip.rs`, `core/crates/core/src/caddyfile/normalise.rs` | 10 | 13.9 |
| 13.12 | `ImportFromCaddyfile` mutation, HTTP endpoints, audit row authoring | `core/crates/core/src/mutation.rs`, `core/crates/cli/src/http/imports.rs`, `core/crates/core/src/audit.rs` | 6 | 13.7, 13.8, 13.11 |
| 13.13 | Web UI: Import wizard, `LossyWarningList`, `MutationPreviewList` | `web/src/features/caddyfile-import/{ImportWizard,MutationPreviewList}.tsx`, `web/src/components/LossyWarningList.tsx` | 8 | 13.12 |

After every slice: `cargo build --workspace` succeeds; `pnpm typecheck` succeeds where the slice touches the web; the slice's named tests pass.

---

## Slice 13.1 [standard] — Lexer: token types and `lex` function

### Goal

Ship the Caddyfile lexer in pure `core`. The lexer consumes a `&str`, emits a `Vec<Token>` with `Span` metadata for every token, handles backslash continuations, double- and backtick-quoted strings with the escape set `\\`, `\n`, `\t`, `\"`, inline `#` comments, environment substitutions (`{$VAR}`, `{$VAR:default}`), and Caddy placeholders (`{http.request.host}`, treated as opaque). Errors carry line and column.

### Entry conditions

- Phase 12 complete.
- `dev-dependencies` in `core/crates/core/Cargo.toml` include `pretty_assertions`, `insta`, `proptest`.
- `core/crates/core/src/caddyfile/mod.rs` exists (create if absent) and is registered from `lib.rs`.

### Files to create or modify

- `core/crates/core/src/caddyfile/mod.rs` — register `pub mod lexer;`.
- `core/crates/core/src/caddyfile/lexer.rs` — token types, `lex`, `LexError`.
- `core/crates/core/src/lib.rs` — add `pub mod caddyfile;` if not yet exported.

### Signatures and shapes

```rust
// core/crates/core/src/caddyfile/lexer.rs

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(String),
    String(String),
    OpenBrace,
    CloseBrace,
    Newline,
    Comment(String),
    EnvSubstitution { name: String, default: Option<String> },
    Placeholder { path: String },
    LineContinuation,
    EndOfFile,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub line: u32,
    pub column: u32,
    pub byte_offset: u32,
    pub byte_length: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> { pub value: T, pub span: Span }

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LexError {
    #[error("unterminated string at line {line}, column {column}")]
    UnterminatedString { line: u32, column: u32 },
    #[error("unbalanced brace at line {line}, column {column}")]
    UnbalancedBrace { line: u32, column: u32 },
    #[error("invalid escape sequence \\{ch} at line {line}, column {column}")]
    InvalidEscape { ch: char, line: u32, column: u32 },
    #[error("invalid placeholder at line {line}, column {column}: {detail}")]
    InvalidPlaceholder { line: u32, column: u32, detail: String },
    #[error("input is not valid UTF-8")]
    NotUtf8,
}

pub fn lex(input: &str) -> Result<Vec<Spanned<Token>>, LexError>;
```

### Algorithm

The lexer is a hand-written character scanner with one lookahead character.

1. Initialise `line = 1`, `column = 1`, `byte_offset = 0`, `tokens = Vec::new()`.
2. Loop until end of input:
   1. Skip horizontal whitespace (space, tab) updating `column`.
   2. On `\n`, emit `Newline`, reset `column = 1`, advance `line`.
   3. On `\\\n` (backslash followed by newline), emit `LineContinuation`, advance both pointers without emitting `Newline`.
   4. On `#` outside a string, scan until newline; emit `Comment(body)`.
   5. On `"` or `` ` ``, scan a quoted string. For double-quoted strings, recognise the escape set `\\`, `\n`, `\t`, `\"`. Backtick strings are raw. Unterminated → `UnterminatedString`. Invalid escape → `InvalidEscape`.
   6. On `{`, peek the next character. If `$`, scan an `EnvSubstitution`: capture the name `[A-Za-z_][A-Za-z0-9_]*`; on `:`, capture the default until `}`; on `}`, emit. If the next character is alphanumeric or dot-separated, scan a `Placeholder`: capture `[A-Za-z0-9_.]+` until `}`; emit `Placeholder`. Otherwise emit `OpenBrace`.
   7. On `}`, emit `CloseBrace`.
   8. Otherwise read until the next whitespace, brace, or newline; emit `Word(s)`.
3. Emit `EndOfFile` and return.

Brace balance tracking is performed at the parser layer, not the lexer (the lexer only emits `OpenBrace` and `CloseBrace`); `UnbalancedBrace` here applies only to braces inside placeholder/env substitution scanning where the body is unterminated.

### Tests

Unit tests inline in `core/crates/core/src/caddyfile/lexer.rs` under `mod tests`:

- `token_types` — instantiate every variant; assert the trait derivations (`Debug`, `Clone`, `PartialEq`).
- `lex_simple_word` — input `"hello"`; assert one `Word("hello")` plus `EndOfFile`.
- `lex_double_quoted_string_with_escapes` — input `"\"a\\nb\""`; assert one `String("a\nb")`.
- `lex_backtick_string_is_raw` — input `` "`a\\nb`" ``; assert `String("a\\nb")`.
- `lex_inline_comment` — `"foo # bar\nbaz"`; assert `Word("foo")`, `Comment(" bar")`, `Newline`, `Word("baz")`.
- `lex_env_substitution_with_default` — input `"{$HOST:localhost}"`; assert `EnvSubstitution { name: "HOST", default: Some("localhost") }`.
- `lex_env_substitution_no_default` — input `"{$HOST}"`; assert `EnvSubstitution { name: "HOST", default: None }`.
- `lex_placeholder_passthrough` — input `"{http.request.host}"`; assert `Placeholder { path: "http.request.host" }`.
- `lex_line_continuation_joins_lines` — input `"a \\\nb"`; assert `Word("a")`, `LineContinuation`, `Word("b")` and that `line` advances correctly on the spanned offsets.
- `lex_unterminated_string_returns_error` — input `"\"unterminated"`; assert `LexError::UnterminatedString { line: 1, column: <col> }`.
- `lex_invalid_escape_returns_error` — input `"\"\\q\""`; assert `LexError::InvalidEscape { ch: 'q', .. }`.
- `lex_proptest_no_panic_on_arbitrary_utf8` — `proptest!` generating arbitrary `String` values; assert `lex` either returns `Ok` or `Err`, never panics.

### Acceptance command

`cargo test -p trilithon-core caddyfile::lexer::tests`

### Exit conditions

- All twelve tests pass.
- `cargo build -p trilithon-core` succeeds.
- The lexer is pure (`#[forbid(unsafe_code)]`, no I/O, no async).

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.5.
- ADR-0002.
- Phase reference §"Lexer (core)".

---

## Slice 13.2 [cross-cutting] — Lexer fuzz harness

### Goal

Ship a `cargo fuzz` target exercising `lex` with arbitrary byte input. The harness MUST run for at least 60 seconds in CI without producing a crash or timeout (per the phase reference and hazard H15).

### Entry conditions

- Slice 13.1 complete.
- `cargo fuzz` toolchain available locally (libFuzzer).

### Files to create or modify

- `core/crates/core/fuzz/Cargo.toml` — fuzz subcrate manifest.
- `core/crates/core/fuzz/fuzz_targets/caddyfile_lexer.rs` — target.
- `.github/workflows/fuzz.yml` (or extend existing CI) to run the target for 60 seconds.

### Signatures and shapes

```rust
// core/crates/core/fuzz/fuzz_targets/caddyfile_lexer.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = trilithon_core::caddyfile::lexer::lex(s);
    }
});
```

### Algorithm

The fuzz target is intentionally trivial: convert the byte slice to a `&str` if valid UTF-8, then call `lex`. Any panic, infinite loop, or timeout is a defect.

### Tests

- `cargo +nightly fuzz run caddyfile_lexer -- -max_total_time=60` returns exit code zero.

CI integration uses the same command in a job pinned to the `nightly-2026-04-01` toolchain.

### Acceptance command

`cargo +nightly fuzz run caddyfile_lexer -- -max_total_time=60`

### Exit conditions

- The fuzz job runs 60 seconds in CI without a crash.
- Any future regression that introduces a panic on adversarial input is caught by the CI job.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Hazards H15.
- PRD T1.5.

---

## Slice 13.3 [standard] — Parser AST types and recursive-descent `parse`

### Goal

Define the AST node types and ship a recursive-descent parser that consumes the lexer's `Vec<Spanned<Token>>` and produces a `CaddyfileAst`. The parser tracks nesting depth against `ParseOptions::max_nesting_depth` and emits `ParseError::Structural { line, column, message }` with line/column for syntax errors. AST types implement `serde::Serialize` for golden-file dumping.

### Entry conditions

- Slice 13.1 complete.

### Files to create or modify

- `core/crates/core/src/caddyfile/ast.rs` — node types.
- `core/crates/core/src/caddyfile/parser.rs` — `parse`, `ParseOptions`, `ParseError`.
- `core/crates/core/src/caddyfile/mod.rs` — register the new modules.

### Signatures and shapes

```rust
// core/crates/core/src/caddyfile/ast.rs
use std::collections::HashMap;
use std::path::PathBuf;
use crate::caddyfile::lexer::Span;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Address {
    pub scheme: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Argument {
    Word(String),
    String(String),
    Env { name: String, default: Option<String> },
    Placeholder { path: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Directive {
    pub name: String,
    pub args: Vec<Argument>,
    pub body: Option<Vec<Directive>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MatcherClause {
    pub kind: String,
    pub args: Vec<Argument>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MatcherDefinition {
    pub name: String,
    pub body: Vec<MatcherClause>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SiteBlock {
    pub addresses: Vec<Address>,
    pub body: Vec<Directive>,
    pub matchers: Vec<MatcherDefinition>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum ImportTarget {
    File(PathBuf),
    Snippet(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Snippet {
    pub name: String,
    pub body: Vec<Directive>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaddyfileAst {
    pub globals: Vec<Directive>,
    pub snippets: HashMap<String, Snippet>,
    pub sites: Vec<SiteBlock>,
}
```

```rust
// core/crates/core/src/caddyfile/parser.rs
use crate::caddyfile::ast::CaddyfileAst;
use crate::caddyfile::lexer::{Spanned, Token};

#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub max_nesting_depth: u32,
}

impl Default for ParseOptions {
    fn default() -> Self { Self { max_nesting_depth: 32 } }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParseError {
    #[error("structural error at line {line}, column {column}: {message}")]
    Structural { line: u32, column: u32, message: String },
    #[error("nesting depth {observed} exceeds maximum {allowed}")]
    NestingExceeded { observed: u32, allowed: u32 },
    #[error("snippet name {name} declared twice (line {line})")]
    DuplicateSnippet { name: String, line: u32 },
}

pub fn parse(tokens: &[Spanned<Token>], opts: &ParseOptions) -> Result<CaddyfileAst, ParseError>;
```

### Algorithm

The parser is recursive-descent with explicit depth tracking. Top-level grammar:

```
caddyfile     := (global | snippet | site_block | newline)*
global        := directive            // top-level directive whose name is in the global set
snippet       := "(" word ")" "{" directive* "}"
site_block    := address ("," address)* "{" (matcher_def | directive)* "}"
matcher_def   := "@" word ( "{" matcher_clause* "}" | matcher_clause )
directive     := word arg* ( "{" directive* "}" )?
```

1. Initialise a cursor `i = 0` over `tokens`, depth `d = 0`.
2. Skip leading `Newline` and `Comment` tokens. Comments do not appear in the AST; the lossy-warning path emits `CommentLoss` at translation time (slice 13.6).
3. While the cursor has tokens, peek:
   1. If `(`-word-`)`: parse a snippet definition. Insert into `ast.snippets`; reject duplicates with `DuplicateSnippet`.
   2. Else if the first non-comment token of the line is a Caddy global directive name (closed list: `auto_https`, `email`, `default_sni`, `log`, `servers`, `storage`, `acme_ca`, `admin`, `debug`, `grace_period`, `order`): parse as a global directive at depth 0.
   3. Else parse a site block. Address parsing: comma-separated list of addresses on the line preceding the opening `{`.
4. Inside a brace body, `d += 1`; if `d > opts.max_nesting_depth`, return `Err(NestingExceeded { observed: d, allowed: opts.max_nesting_depth })`.
5. Inside a site block, `@name { ... }` enters matcher-definition mode; collect clauses into a `MatcherDefinition`.
6. Each directive parses its name (a `Word`), then arguments until `Newline`, then optionally a brace body. Sub-bodies recurse.
7. Brace mismatches surface as `Structural { line, column, message }` carrying the offending line.

### Tests

Unit tests at `core/crates/core/src/caddyfile/parser.rs`:

- `parse_single_site_no_body` — input `"example.com\n"`; assert one site, no body.
- `parse_single_site_with_reverse_proxy` — `"example.com {\n  reverse_proxy localhost:8080\n}\n"`; assert one site, one directive named `reverse_proxy` with one `Word("localhost:8080")` argument.
- `parse_snippet_definition` — `"(common) { header X-A 1 }\nexample.com { import common }"`; assert `ast.snippets["common"]` exists.
- `parse_named_matcher` — site block containing `@api { path /api/* }`; assert `MatcherDefinition` with kind `path`.
- `parse_nested_block_within_depth_passes` — 31 nested braces; assert `Ok`.
- `parse_nested_block_exceeds_depth_fails` — 33 nested braces; assert `Err(NestingExceeded { observed: 33, allowed: 32 })`.
- `parse_unbalanced_brace_returns_structural_error_with_line_column`.
- `parse_duplicate_snippet_returns_error`.
- `ast_serializes_to_json` — fixture AST round-trips through `serde_json` to JSON and back.

### Acceptance command

`cargo test -p trilithon-core caddyfile::parser::tests caddyfile::ast::tests`

### Exit conditions

- All nine tests pass.
- AST types implement `serde::Serialize` for golden snapshotting.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.5.
- Phase reference §"Parser (core)".

---

## Slice 13.4 [cross-cutting] — Snippet expander, environment resolver, file-import resolver

### Goal

Three separate post-parse passes:

1. `expand` inlines `import <snippet>` directives, detects cycles, tracks expansion factor.
2. `resolve_env` replaces every `EnvSubstitution` argument with its resolved string (consulting `EnvProvider`), emitting `LossyWarning::EnvSubstitutionEmpty` for unresolved variables without a default.
3. `resolve_file_imports` resolves relative paths against a configured `root`, refuses traversal outside `root`, and detects file-import cycles.

### Entry conditions

- Slice 13.3 complete.
- `core::config::EnvProvider` is available per trait-signatures.md §9.

### ⚠️ Core-purity constraint

`resolve_file_imports` (step 3 of this slice) performs real filesystem I/O — `canonicalize` and file reads. **This is forbidden in `core/`** (architecture §4.1: `core` must have no I/O). The correct pattern, matching `EnvProvider` (trait-signatures.md §9), is:

1. Define a `FileImportReader` trait in `core/crates/core/src/caddyfile/import.rs`:
   ```rust
   pub trait FileImportReader: Send + Sync {
       fn read(&self, canonical_path: &Path) -> Result<String, ImportError>;
       fn canonicalize(&self, root: &Path, relative: &Path) -> Result<PathBuf, ImportError>;
   }
   ```
2. Accept `&dyn FileImportReader` in `resolve_file_imports` — keeping the logic pure.
3. Implement `TokioFileImportReader` in `core/crates/adapters/src/caddyfile_file_import.rs` using `std::fs`.

The file listed below (`import.rs` in `core`) holds the trait + pure logic only. The adapter implementation is an additional file the slice must create.

### Files to create or modify

- `core/crates/core/src/caddyfile/expand.rs`.
- `core/crates/core/src/caddyfile/env.rs`.
- `core/crates/core/src/caddyfile/import.rs` — `FileImportReader` trait + pure logic.
- `core/crates/adapters/src/caddyfile_file_import.rs` — `TokioFileImportReader` implementing `FileImportReader` with real filesystem I/O.
- `core/crates/core/src/caddyfile/lossy.rs` — provisional `LossyWarning` enum (full version lands in slice 13.6).

### Signatures and shapes

```rust
// core/crates/core/src/caddyfile/expand.rs
use crate::caddyfile::ast::CaddyfileAst;

#[derive(Debug, Clone)]
pub struct ExpandOptions {
    pub max_snippet_expansion: u32,   // default 100, expanded_bytes / source_bytes
    pub max_recursion_depth: u32,     // default 32
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExpandError {
    #[error("snippet import cycle: {chain:?}")]
    ImportCycle { chain: Vec<String> },
    #[error("snippet {name} not defined")]
    SnippetUndefined { name: String },
    #[error("snippet expansion factor {factor}x exceeds limit {allowed}x")]
    ExpansionExceeded { factor: u32, allowed: u32 },
}

pub fn expand(ast: CaddyfileAst, opts: &ExpandOptions) -> Result<CaddyfileAst, ExpandError>;
```

```rust
// core/crates/core/src/caddyfile/env.rs
use crate::caddyfile::ast::CaddyfileAst;
use crate::caddyfile::lossy::LossyWarning;
use crate::config::EnvProvider;

pub fn resolve_env(ast: &mut CaddyfileAst, env: &dyn EnvProvider) -> Vec<LossyWarning>;
```

```rust
// core/crates/core/src/caddyfile/import.rs
use std::path::Path;
use crate::caddyfile::ast::CaddyfileAst;

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub max_recursion_depth: u32,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ImportError {
    #[error("path {path} escapes root {root}")]
    PathTraversal { path: String, root: String },
    #[error("file-import cycle: {chain:?}")]
    ImportCycle { chain: Vec<String> },
    #[error("imported file {path} not readable: {detail}")]
    Unreadable { path: String, detail: String },
    #[error("input size {observed} bytes exceeds limit {allowed}")]
    SizeExceeded { kind: &'static str, observed: u64, allowed: u64 },
}

pub fn resolve_file_imports(
    ast: CaddyfileAst,
    root: &Path,
    reader: &dyn FileImportReader,
    opts: &ImportOptions,
) -> Result<CaddyfileAst, ImportError>;
```

### Algorithm — `expand`

1. Build `referenced: HashMap<String, &Snippet>` from `ast.snippets`.
2. Recursively walk every `Directive` tree (`globals` + `sites[*].body`). On a directive whose name is `import` and whose first argument resolves to a snippet name in `referenced`:
   1. Push the snippet name onto a `chain: Vec<String>`. If already present, return `Err(ImportCycle { chain })`.
   2. Track `source_bytes` (length of the original AST's `serde_json` form) and `expanded_bytes` (length after substitution). If `expanded_bytes / source_bytes > opts.max_snippet_expansion`, return `Err(ExpansionExceeded)`.
   3. Splice the snippet body into the calling directive's parent.
   4. Pop from `chain`.
3. After all expansions, drop the `snippets` map; the resulting AST has no remaining `import <snippet>` directives.

### Algorithm — `resolve_env`

1. Walk every `Argument` in every `Directive`.
2. For each `Argument::Env { name, default }`, call `env.var(&name)`.
3. On `Ok(value)`, replace with `Argument::Word(value)`.
4. On `Err(EnvError::NotPresent)`:
   1. If `default.is_some()`, replace with `Argument::Word(default.unwrap())`.
   2. Otherwise replace with `Argument::Word(String::new())` and push `LossyWarning::EnvSubstitutionEmpty { name }`.
5. Return the warning list.

### Algorithm — `resolve_file_imports`

1. Walk every `import` directive whose first argument resolves to a path (not a snippet name).
2. Resolve the path relative to `root`. Canonicalise via `std::path::Path::canonicalize`. If the canonicalised path does not start with `root`, return `Err(PathTraversal)`.
3. Push the canonicalised path onto a `chain`. If already present, return `Err(ImportCycle)`.
4. Read the file (size-bounded by `ImportOptions`). Lex, parse, and recursively resolve.
5. Splice the parsed AST's `globals` and `sites` into the importing context.

### Tests

Unit tests in each module:

- `expand::tests::cycle_detected` — `(a) { import b }` and `(b) { import a }` plus a site importing `a`; assert `Err(ImportCycle)`.
- `expand::tests::expansion_factor_bound` — snippet that produces 200× expansion; assert `Err(ExpansionExceeded)`.
- `expand::tests::happy_path` — single snippet imported once; assert the resulting AST contains the snippet's body inline.
- `env::tests::warning_for_unresolved_no_default`.
- `env::tests::no_warning_for_unresolved_with_default`.
- `env::tests::happy_path_resolves`.
- `import::tests::path_traversal_rejected` — argument `../etc/passwd` against a sandboxed root; assert `Err(PathTraversal)`.
- `import::tests::cycle_detected` — `a.caddyfile` imports `b.caddyfile` imports `a.caddyfile`; assert `Err(ImportCycle)`.
- `import::tests::happy_path` — `imports/site.caddyfile` is included.

### Acceptance command

`cargo test -p trilithon-core caddyfile::expand::tests caddyfile::env::tests caddyfile::import::tests`

### Exit conditions

- All nine tests pass.
- Cycle detection is deterministic.
- Path traversal is refused for any input that would resolve outside `root`.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.5.
- Hazards H15.
- Trait signatures §9 (`EnvProvider`).

---

## Slice 13.5 [standard] — Size guards (input, directives, nesting, expansion factor, route count)

### Goal

Five separate size bounds, gathered into `core::caddyfile::limits`:

1. `check_input_size` rejects inputs over 5 MiB before lexing.
2. The parser tracks running directive count and aborts at 10,000.
3. The parser already enforces nesting depth at 32 (slice 13.3); this slice adds the typed `ImportError::SizeExceeded` reporting variant.
4. `expand` enforces an expansion factor of 100× (slice 13.4).
5. The translator counts produced `CreateRoute` mutations and aborts past 5,000.

Bound (1) is reused by HTTP imports. The pathological-fixture batch in slice 13.10 verifies all five.

### Entry conditions

- Slices 13.3 and 13.4 complete.

### Files to create or modify

- `core/crates/core/src/caddyfile/limits.rs` — `SizeOptions`, `check_input_size`, helper counters, `ImportError::SizeExceeded`.
- `core/crates/core/src/caddyfile/parser.rs` — wire directive-count counter.
- `core/crates/core/src/caddyfile/translator/mod.rs` — wire route-count counter (anticipates slice 13.6).

### Signatures and shapes

```rust
// core/crates/core/src/caddyfile/limits.rs
#[derive(Debug, Clone)]
pub struct SizeOptions {
    pub max_input_bytes: u64,         // default 5 * 1024 * 1024
    pub max_directives: u32,          // default 10_000
    pub max_nesting_depth: u32,       // default 32
    pub max_snippet_expansion: u32,   // default 100
    pub max_routes: u32,              // default 5_000
}

impl Default for SizeOptions { /* values above */ }

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SizeError {
    #[error("input is {observed} bytes; limit is {allowed}")]
    InputTooLarge { observed: u64, allowed: u64 },
    #[error("{kind} count {observed} exceeds limit {allowed}")]
    SizeExceeded { kind: &'static str, observed: u32, allowed: u32 },
}

pub fn check_input_size(bytes: &[u8], opts: &SizeOptions) -> Result<(), SizeError>;
```

### Algorithm

`check_input_size` is constant-time: compare `bytes.len() as u64` to `opts.max_input_bytes`. The other counters are integer increments inside the existing parse / expand / translate loops; each compares to the configured ceiling and emits the typed error on overflow.

### Tests

Integration tests at `core/crates/core/tests/caddyfile_size_guards.rs`:

- `input_size_guard_rejects_6_mib_blob_within_100_ms` — generate a 6 MiB blob; assert `check_input_size` returns `Err(InputTooLarge)` and the wall-clock duration is under 100 milliseconds.
- `directive_count_guard_rejects_synthetic_12000_within_2_s` — generate 12,000 sites with one directive each; assert rejection within two seconds.
- `nesting_depth_guard_rejects_33_levels`.
- `route_count_guard_rejects_6000_sites`.

The `expansion_factor_bound` test from slice 13.4 satisfies the fourth bound.

### Acceptance command

`cargo test -p trilithon-core --test caddyfile_size_guards`

### Exit conditions

- All four tests pass within their stated time budgets.
- Resident memory during each test stays under 256 MiB (asserted in slice 13.10's pathological-fixture test).

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Hazards H15.
- PRD T1.5.

---

## Slice 13.6 [standard] — `LossyWarning` catalogue and translator dispatch skeleton

### Goal

Define the closed `LossyWarning` enum with stable kebab-case identifiers and ship the translator dispatch skeleton. The dispatch routes each `Directive` to a handler-specific translator (slices 13.7 and 13.8) and emits `LossyWarning::UnsupportedDirective` for any unhandled directive without aborting.

### Entry conditions

- Slice 13.4 complete.

### Files to create or modify

- `core/crates/core/src/caddyfile/lossy.rs` — `LossyWarning`, `id`, `LossyWarningSet`.
- `core/crates/core/src/caddyfile/translator/mod.rs` — `Translator` orchestration.
- `core/crates/core/src/caddyfile/translator/dispatch.rs` — directive dispatch table.

### Signatures and shapes

```rust
// core/crates/core/src/caddyfile/lossy.rs
use crate::caddyfile::lexer::Span;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "id", rename_all = "kebab-case")]
pub enum LossyWarning {
    UnsupportedDirective { directive: String, span: Span },
    CommentLoss { span: Span },
    OrderingLoss { detail: String, span: Span },
    SnippetExpansionLoss { snippet: String },
    EnvSubstitutionEmpty { name: String },
    TlsDnsProviderUnavailable { provider: String, span: Span },
    PlaceholderPassthrough { path: String, span: Span },
    CapabilityDegraded { capability: String, span: Span },
}

impl LossyWarning {
    pub fn id(&self) -> &'static str {
        match self {
            Self::UnsupportedDirective { .. } => "unsupported-directive",
            Self::CommentLoss { .. } => "comment-loss",
            Self::OrderingLoss { .. } => "ordering-loss",
            Self::SnippetExpansionLoss { .. } => "snippet-expansion-loss",
            Self::EnvSubstitutionEmpty { .. } => "env-substitution-empty",
            Self::TlsDnsProviderUnavailable { .. } => "tls-dns-provider-unavailable",
            Self::PlaceholderPassthrough { .. } => "placeholder-passthrough",
            Self::CapabilityDegraded { .. } => "capability-degraded",
        }
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct LossyWarningSet {
    pub warnings: Vec<LossyWarning>,
}
```

```rust
// core/crates/core/src/caddyfile/translator/mod.rs
use crate::caddyfile::ast::CaddyfileAst;
use crate::caddyfile::lossy::LossyWarningSet;
use crate::caddyfile::limits::SizeOptions;
use crate::caddy::CapabilitySet;
use crate::mutation::TypedMutation;

pub mod dispatch;
pub mod sites;
// ... other handler modules registered in slices 13.7 / 13.8

pub struct TranslateContext<'a> {
    pub capabilities: &'a CapabilitySet,
    pub limits: &'a SizeOptions,
}

#[derive(Debug, Default)]
pub struct TranslateResult {
    pub mutations: Vec<TypedMutation>,
    pub warnings: LossyWarningSet,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TranslateError {
    #[error("size exceeded for {kind}: observed {observed}, allowed {allowed}")]
    SizeExceeded { kind: &'static str, observed: u32, allowed: u32 },
    #[error("malformed directive {name}: {detail}")]
    Malformed { name: String, detail: String },
}

pub fn translate(
    ast: &CaddyfileAst,
    ctx: &TranslateContext<'_>,
) -> Result<TranslateResult, TranslateError>;
```

### Algorithm

1. Initialise `result = TranslateResult::default()`.
2. For each `SiteBlock` in `ast.sites`, dispatch to `sites::translate_site` (slice 13.7) which returns an iterator of mutations and warnings.
3. After translating all sites, count `result.mutations.iter().filter(|m| matches!(m, TypedMutation::CreateRoute(_))).count()`. If `> ctx.limits.max_routes`, return `Err(SizeExceeded { kind: "routes", .. })`.
4. Iterate `ast.globals` for any directive whose name is not in the closed global set; emit `UnsupportedDirective`.
5. The dispatch table maps directive name → handler function. Unknown directives go through `dispatch::emit_unsupported` which pushes a single `UnsupportedDirective` warning and returns no mutations.

### Tests

Unit tests at `core/crates/core/src/caddyfile/lossy.rs`:

- `lossy_warning_ids_are_unique` — assert every variant's `id()` is distinct.

Unit tests at `core/crates/core/src/caddyfile/translator/dispatch.rs`:

- `dispatch_emits_unsupported_for_synthetic_php_fastcgi_directive` — fixture AST containing a `php_fastcgi` directive; assert one `LossyWarning::UnsupportedDirective` and translation completes (no error).

### Acceptance command

`cargo test -p trilithon-core caddyfile::lossy::tests caddyfile::translator::dispatch::tests`

### Exit conditions

- Both tests pass.
- The eight `LossyWarning` variants have stable kebab-case identifiers.
- The dispatch skeleton is callable; every concrete handler registered in 13.7 / 13.8 plugs in via the dispatch table.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.5.

---

## Slice 13.7 [standard] — Translator batch A: sites, `reverse_proxy`, `file_server`, `redir`, `respond`

### Goal

Concrete translators for the "carrying" directives: site-block-to-routes, reverse-proxy upstreams, file server, redirect, static response. Each handler is a `pub fn` that receives the `Directive`, the surrounding `TranslateContext`, and a mutable `TranslateResult`, and emits the appropriate typed mutations or `LossyWarning`.

### Entry conditions

- Slice 13.6 complete.
- The mutation algebra (Phase 4) exposes `CreateRoute`, `SetUpstream`, `SetFileServer`, `SetRedirect`, `SetStaticResponse`, `SetMatchers`.

### Files to create or modify

- `core/crates/core/src/caddyfile/translator/sites.rs` — `translate_site`.
- `core/crates/core/src/caddyfile/translator/reverse_proxy.rs`.
- `core/crates/core/src/caddyfile/translator/file_server.rs`.
- `core/crates/core/src/caddyfile/translator/redir.rs`.
- `core/crates/core/src/caddyfile/translator/respond.rs`.

### Signatures and shapes

Each handler takes the same shape; for example:

```rust
// core/crates/core/src/caddyfile/translator/reverse_proxy.rs
use crate::caddyfile::ast::Directive;
use crate::caddyfile::translator::{TranslateContext, TranslateResult};

pub fn translate_reverse_proxy(
    directive: &Directive,
    ctx: &TranslateContext<'_>,
    out: &mut TranslateResult,
);
```

`sites::translate_site` returns `()` and mutates `out`. Internally it generates a stable route id (ULID) per resolved address, emits a `CreateRoute` mutation, then dispatches each child directive through the dispatch table.

### Algorithm — `translate_site`

1. For each `Address` in `site.addresses`, generate a deterministic route id from the canonicalised address string (use `Ulid::from_string(blake3(addr).to_hex()[..26])` or, alternatively, a fresh ULID; either is acceptable provided the same address yields the same id within one translation pass).
2. Emit `CreateRoute { id, host: address.host, port: address.port.unwrap_or(443) }`.
3. For every `MatcherDefinition` in `site.matchers`, emit `SetMatchers { route_id, name, clauses }`.
4. For every `Directive` in `site.body`, dispatch.

### Algorithm — `translate_reverse_proxy`

1. Read the positional arguments after `reverse_proxy`: each is an upstream destination (`host:port` or `unix//path`).
2. Read the brace body for sub-directives: `to`, `lb_policy`, `health_uri`, `header_up`, `header_down`, `transport`.
3. `to`: append upstreams.
4. `lb_policy`: parse policy name (round-robin, random, ip-hash, header). Unknown policy emits `UnsupportedDirective`.
5. `health_uri`: capture the path.
6. `header_up`/`header_down`: parse `set <name> <value>`, `add`, `delete` and accumulate.
7. `transport http { ... }`: parse TLS subdirectives (`tls`, `tls_insecure_skip_verify`, `versions`) and H2C (`h2c`).
8. Any unknown sub-directive emits `UnsupportedDirective`.
9. Emit `SetUpstream { route_id, upstreams, lb_policy, health_uri, transport, headers_up, headers_down }`.

### Algorithm — `translate_file_server`

1. Read brace body sub-directives: `root`, `browse`, `hide`, `index`.
2. `root`: capture path.
3. `browse`: capture the templating path (optional argument); record boolean.
4. `hide`: collect the list of patterns.
5. `index`: collect index file names.
6. Emit `SetFileServer { route_id, root, browse, hide, index }`.

### Algorithm — `translate_redir`

1. Read positional arguments: `<target> [status]`.
2. `status` defaults to `302` when omitted; parse as integer.
3. Emit `SetRedirect { route_id, target, status }`.

### Algorithm — `translate_respond`

1. Parse `respond <status> <body>?` with optional `close` flag.
2. Emit `SetStaticResponse { route_id, status, body, close }`.

### Tests

Unit tests live alongside each handler. Names:

- `sites::tests::single_host_emits_create_route_with_host_and_default_port_443`.
- `sites::tests::single_host_with_port_overrides_default`.
- `reverse_proxy::tests::single_upstream`.
- `reverse_proxy::tests::multiple_upstreams_with_lb_policy`.
- `reverse_proxy::tests::healthcheck_path_recorded`.
- `reverse_proxy::tests::transport_http_tls_versions_recorded`.
- `reverse_proxy::tests::unknown_lb_policy_emits_unsupported_directive`.
- `file_server::tests::root_browse_hide_index_all_recorded`.
- `redir::tests::status_defaults_to_302_when_omitted`.
- `respond::tests::body_and_close_flag_recorded`.

### Acceptance command

`cargo test -p trilithon-core caddyfile::translator::sites::tests caddyfile::translator::reverse_proxy::tests caddyfile::translator::file_server::tests caddyfile::translator::redir::tests caddyfile::translator::respond::tests`

### Exit conditions

- All ten tests pass.
- Each handler is registered in the dispatch table.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.5, T1.6.

---

## Slice 13.8 [standard] — Translator batch B: handlers, headers, TLS, encode, log, matchers

### Goal

Concrete translators for the routing-control directives. `route`, `handle`, `handle_path` produce ordered handler lists with `OrderingLoss` warnings on ambiguity. `header` produces `SetHeaders`. `tls` produces `SetTls`, with the DNS-provider variant emitting `TlsDnsProviderUnavailable` when the capability cache reports the provider missing. `encode` translates `gzip` and `zstd` (others emit `UnsupportedDirective`). `log` translates `output`, `format`, `include`, `exclude`. Named matchers (`@name { ... }`) translate `path`, `path_regexp`, `host`, `header`, `method`, `query`, `expression`, `not`, `protocol`, `remote_ip`, `client_ip`.

### Entry conditions

- Slice 13.7 complete.

### Files to create or modify

- `core/crates/core/src/caddyfile/translator/handlers.rs`.
- `core/crates/core/src/caddyfile/translator/headers.rs`.
- `core/crates/core/src/caddyfile/translator/tls.rs`.
- `core/crates/core/src/caddyfile/translator/encode.rs`.
- `core/crates/core/src/caddyfile/translator/log.rs`.
- `core/crates/core/src/caddyfile/translator/matchers.rs`.

### Signatures and shapes

```rust
// core/crates/core/src/caddyfile/translator/handlers.rs
pub fn translate_route(directive: &Directive, ctx: &TranslateContext<'_>, out: &mut TranslateResult);
pub fn translate_handle(directive: &Directive, ctx: &TranslateContext<'_>, out: &mut TranslateResult);
pub fn translate_handle_path(directive: &Directive, ctx: &TranslateContext<'_>, out: &mut TranslateResult);
```

The remaining handlers follow the same `pub fn (&Directive, &TranslateContext, &mut TranslateResult)` shape.

### Algorithm — handlers

1. `handle <matcher>? { ... }` produces a handler block. The translator emits a `SetMatchers` reference (named matcher already translated) plus the inner directives in declaration order.
2. `handle_path <prefix>` synthesises a `path` matcher with `<prefix>*` and recurses as `handle`.
3. `route` is an explicit ordering wrapper. If a `route` contains nested `route` blocks whose matchers are not orthogonal (the translator detects overlap by computing a path-prefix intersection), emit `LossyWarning::OrderingLoss`.

### Algorithm — headers

1. Walk the directive body. Each clause is one of `<name> <value>`, `>name value` (replacement), `+name value` (add), `-name` (delete).
2. For request headers (`header_up`) versus response headers (`header_down`), branch.
3. Emit `SetHeaders { route_id, request: HeadersOps, response: HeadersOps }`.

### Algorithm — TLS

1. Parse subdirectives: `internal`, `<email>` (email issuer), `<cert_file> <key_file>`, `dns <provider>`, `protocols`, `ciphers`, `curves`, `alpn`.
2. For `dns <provider>`, look up the capability cache: `ctx.capabilities.has_module(&format!("dns.providers.{provider}"))`. If absent, push `LossyWarning::TlsDnsProviderUnavailable { provider, span }` and skip the DNS-provider option (the resulting `SetTls` falls back to internal issuance).
3. Emit `SetTls { route_id, mode, email, cert_file, key_file, dns_provider, protocols, ciphers, curves, alpn }`.

### Algorithm — encode

1. Read positional arguments. For each algorithm:
   1. `gzip` → push to `encodings`.
   2. `zstd` → push to `encodings`.
   3. Any other → emit `UnsupportedDirective`.
2. Emit `SetEncoding { route_id, encodings }`.

### Algorithm — log

1. Parse subdirectives: `output <path>` or `output stdout`, `format json|console`, `include <namespaces>`, `exclude <namespaces>`.
2. Emit `SetLog { route_id, output, format, include, exclude }`.

### Algorithm — matchers

For each `MatcherClause` inside a `MatcherDefinition`:

1. `path <patterns>...` → `Matcher::Path { patterns }`.
2. `path_regexp <name> <pattern>` → `Matcher::PathRegex { name, pattern }`.
3. `host <hosts>...` → `Matcher::Host { hosts }`.
4. `header <name> <value>` → `Matcher::Header`.
5. `method <verbs>...` → `Matcher::Method`.
6. `query <pairs>...` → `Matcher::Query`.
7. `expression <expr>` → `Matcher::Expression`.
8. `not { ... }` → `Matcher::Not(Box<Matcher>)` (recursive).
9. `protocol <proto>` → `Matcher::Protocol`.
10. `remote_ip <cidrs>...` → `Matcher::RemoteIp`.
11. `client_ip <cidrs>...` → `Matcher::ClientIp`.

### Tests

Unit tests:

- `handlers::tests::route_preserves_declaration_order`.
- `handlers::tests::nested_route_with_overlapping_matchers_emits_ordering_loss`.
- `handlers::tests::handle_path_synthesises_path_matcher`.
- `headers::tests::set_add_delete_replacement_all_recorded`.
- `tls::tests::internal_mode`.
- `tls::tests::email_issuer`.
- `tls::tests::explicit_cert_and_key`.
- `tls::tests::dns_provider_emits_tls_dns_provider_unavailable_when_capability_missing`.
- `encode::tests::gzip_and_zstd_translate_brotli_emits_unsupported`.
- `log::tests::output_format_include_exclude_translate`.
- `matchers::tests::all_eleven_matcher_kinds_translate`.

### Acceptance command

`cargo test -p trilithon-core caddyfile::translator::handlers::tests caddyfile::translator::headers::tests caddyfile::translator::tls::tests caddyfile::translator::encode::tests caddyfile::translator::log::tests caddyfile::translator::matchers::tests`

### Exit conditions

- All eleven tests pass.
- The TLS DNS-provider negative test asserts the `LossyWarning::TlsDnsProviderUnavailable` variant.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.5, T1.11 (capability degradation).
- ADR-0013.

---

## Slice 13.9 [standard] — Fixture corpus authoring batches `01_trivial` through `06_snippets`

### Goal

Author the first six fixture batches and the corpus test runner. Each fixture directory contains five files: `caddyfile`, `mutations.golden.json`, `warnings.golden.json`, `caddy-adapt.golden.json`, `requests.ndjson`. The corpus runner asserts that translating `caddyfile` produces the golden mutations and warnings.

### Entry conditions

- Slices 13.7 and 13.8 complete.
- Caddy 2.11.2 binary available locally for `caddy adapt` golden generation.

### Files to create or modify

- `core/crates/core/tests/caddyfile_corpus.rs` — corpus runner.
- `core/crates/core/tests/fixtures/caddyfile/01_trivial/{single_host,single_host_custom_port,single_bare_address_global}/{caddyfile,mutations.golden.json,warnings.golden.json,caddy-adapt.golden.json,requests.ndjson}`.
- `core/crates/core/tests/fixtures/caddyfile/02_reverse_proxy/{single,multi,healthcheck,h2c_transport}/...`.
- `core/crates/core/tests/fixtures/caddyfile/03_virtual_hosts/{two_hosts,three_hosts_distinct_tls,host_port_block}/...`.
- `core/crates/core/tests/fixtures/caddyfile/04_path_matchers/{handle_path_api,named_api,path_regexp_capture}/...`.
- `core/crates/core/tests/fixtures/caddyfile/05_regex_matchers/{path_regexp_backreference,not_path_regexp}/...`.
- `core/crates/core/tests/fixtures/caddyfile/06_snippets/{single_snippet,multi_site_snippet,snippet_imports_snippet}/...`.

### Signatures and shapes

The corpus runner uses `insta` for golden assertions:

```rust
// core/crates/core/tests/caddyfile_corpus.rs
use std::path::Path;
use trilithon_core::caddyfile;

fn run_fixture(dir: &Path) {
    let source = std::fs::read_to_string(dir.join("caddyfile")).expect("caddyfile present");
    let tokens = caddyfile::lexer::lex(&source).expect("lex ok");
    let ast = caddyfile::parser::parse(&tokens, &Default::default()).expect("parse ok");
    let ast = caddyfile::expand::expand(ast, &Default::default()).expect("expand ok");
    let caps = test_capability_set();
    let result = caddyfile::translator::translate(
        &ast,
        &caddyfile::translator::TranslateContext { capabilities: &caps, limits: &Default::default() },
    ).expect("translate ok");
    insta::assert_json_snapshot!(format!("{}-mutations", dir.file_name().unwrap().to_string_lossy()), result.mutations);
    insta::assert_json_snapshot!(format!("{}-warnings", dir.file_name().unwrap().to_string_lossy()), result.warnings);
}

#[test] fn corpus_trivial() { for d in subdirs("01_trivial") { run_fixture(&d); } }
#[test] fn corpus_reverse_proxy() { /* ... */ }
#[test] fn corpus_virtual_hosts() { /* ... */ }
#[test] fn corpus_path_matchers() { /* ... */ }
#[test] fn corpus_regex_matchers() { /* ... */ }
#[test] fn corpus_snippets() { /* ... */ }
```

### Algorithm

For each fixture directory:

1. Read `caddyfile`.
2. Lex, parse, expand, translate.
3. `insta::assert_json_snapshot` against `mutations.golden.json` and `warnings.golden.json`.
4. The `caddy-adapt.golden.json` and `requests.ndjson` files are consumed by the round-trip harness in slice 13.11; this slice verifies their presence only.

### Tests

Six top-level test functions, one per batch, each iterating its subdirectories. Test names match the section list in the phase reference.

### Acceptance command

`cargo test -p trilithon-core --test caddyfile_corpus`

### Exit conditions

- All six corpus test functions pass.
- Every fixture directory contains all five required files.
- The lossy warning for any fixture matches the golden exactly.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.5.

---

## Slice 13.10 [standard] — Fixture corpus authoring batches `07_imports` through `11_pathological`

### Goal

Author the remaining five fixture batches and the pathological-rejection integration test. The pathological batch verifies the size guards from slice 13.5 reject within two seconds and resident memory under 256 MiB on reference hardware.

### Entry conditions

- Slices 13.5 and 13.9 complete.

### Files to create or modify

- `core/crates/core/tests/fixtures/caddyfile/07_imports/{relative_path_file,env_within_import}/...`.
- `core/crates/core/tests/fixtures/caddyfile/08_env_substitution/{resolves,fallback_and_empty}/...`.
- `core/crates/core/tests/fixtures/caddyfile/09_tls/{internal,email_issuer,explicit_cert_key}/...`.
- `core/crates/core/tests/fixtures/caddyfile/10_multi_site_one_file/{ten_sites,twenty_sites_shared_snippet}/...`.
- `core/crates/core/tests/fixtures/caddyfile/11_pathological/{thirty_two_level_nesting,fifteen_thousand_sites,eight_mib_line}/caddyfile`.
- `core/crates/core/tests/caddyfile_pathological.rs` — pathological-rejection runner.
- `core/crates/core/tests/caddyfile_lossy_completeness.rs` — lossy-warning catalogue completeness test.

### Signatures and shapes

```rust
// core/crates/core/tests/caddyfile_pathological.rs
#[test]
fn pathological_thirty_two_level_nesting_rejected_within_two_seconds() {
    let bytes = std::fs::read("tests/fixtures/caddyfile/11_pathological/thirty_two_level_nesting/caddyfile").unwrap();
    let started = std::time::Instant::now();
    let res = trilithon_core::caddyfile::lexer::lex(std::str::from_utf8(&bytes).unwrap())
        .and_then(|toks| trilithon_core::caddyfile::parser::parse(&toks, &Default::default()).map_err(|_| trilithon_core::caddyfile::lexer::LexError::NotUtf8));
    assert!(res.is_err());
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}
```

The other pathological tests follow the same shape and additionally sample resident memory via `getrusage`.

### Algorithm

1. For each pathological fixture, record `started = Instant::now()` and `pre_rss = getrusage()`.
2. Invoke the pipeline (lex → check_input_size → parse → translate, depending on which guard the fixture triggers).
3. Assert the operation returns the typed error within two seconds.
4. Sample `post_rss = getrusage()`. Assert `post_rss < 256 MiB`.

### Tests

`tests/caddyfile_pathological.rs`:

- `pathological_thirty_two_level_nesting_rejected_within_two_seconds`.
- `pathological_fifteen_thousand_sites_rejected_within_two_seconds_and_under_256_mib_rss`.
- `pathological_eight_mib_line_rejected_by_input_size_guard_within_100_ms`.

`tests/caddyfile_lossy_completeness.rs`:

- `every_lossy_warning_variant_appears_in_at_least_one_fixture` — iterate every variant via a generated list, search the `warnings.golden.json` files for that id; assert at least one fixture covers each.

`tests/fixtures/caddyfile/08_env_substitution/fallback_and_empty/warnings.golden.json` MUST contain at least one `env-substitution-empty` entry.

### Acceptance command

`cargo test -p trilithon-core --test caddyfile_pathological --test caddyfile_lossy_completeness`

### Exit conditions

- All four pathological-rejection tests pass within their stated budgets.
- The lossy completeness test passes.
- The `08_env_substitution/fallback_and_empty` fixture asserts the env-empty warning.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Hazards H15.
- PRD T1.5.

---

## Slice 13.11 [cross-cutting] — Round-trip equivalence harness and normalisation rules

### Goal

Ship the seven-step round-trip harness in `core/crates/adapters/tests/caddyfile_round_trip.rs` and the four normalisation `pub fn`s in `core/crates/core/src/caddyfile/normalise.rs`. For each non-pathological fixture, the harness performs:

1. Lex / parse / expand / translate the fixture's Caddyfile.
2. Feed the resulting `DesiredState` to the existing desired-state-to-Caddy-JSON renderer (Phase 4 / Phase 7 substrate).
3. Apply normalisation rules to both Trilithon's JSON and the fixture's `caddy-adapt.golden.json`.
4. Assert byte-for-byte equality of the normalised JSON documents.
5. Boot a real Caddy with each JSON.
6. Replay the fixture's `requests.ndjson`.
7. Assert response equivalence (status, body hash, header subset).

The four normalisation rules are individually unit-tested.

### Entry conditions

- Slices 13.9 and 13.10 complete.
- Caddy 2.11.2 binary on `PATH` (CI bootstrap reads `caddy-version.txt`).
- The Phase 4 / Phase 7 work has produced the `DesiredState` → Caddy JSON renderer.

### Files to create or modify

- `core/crates/core/src/caddyfile/normalise.rs`.
- `core/crates/adapters/tests/caddyfile_round_trip.rs`.
- `core/crates/adapters/tests/helpers/caddy_runner.rs` — small helper to launch and stop a Caddy subprocess.

### Signatures and shapes

```rust
// core/crates/core/src/caddyfile/normalise.rs
use serde_json::Value;

pub fn sort_object_keys(value: &mut Value);
pub fn strip_trilithon_id_annotations(value: &mut Value);
pub fn fold_equivalent_matcher_arrays(value: &mut Value);
pub fn align_automatic_https_disable_redirects(value: &mut Value);

pub fn normalise(value: &mut Value) {
    sort_object_keys(value);
    strip_trilithon_id_annotations(value);
    fold_equivalent_matcher_arrays(value);
    align_automatic_https_disable_redirects(value);
}
```

### Algorithm — round-trip harness

For each non-pathological fixture directory:

1. Translate the `caddyfile` into mutations and apply them in order to an empty `DesiredState`.
2. Render `DesiredState` to Caddy JSON.
3. Read `caddy-adapt.golden.json` (pre-generated against Caddy 2.11.2 via `caddy adapt`).
4. Normalise both documents via `normalise`.
5. Assert byte equality (`pretty_assertions::assert_eq!`).
6. Launch `caddy run --config <trilithon.json>` on a random loopback port. Wait for `/config/` to respond.
7. For each line in `requests.ndjson`, send the request to Caddy. Record `(status, header subset, body hash)`.
8. Stop Caddy. Repeat steps 6 and 7 with `caddy run --config <golden.json>`.
9. Assert the two response sequences match.
10. Stop Caddy.

### Tests

`core/crates/adapters/tests/caddyfile_round_trip.rs`:

- `round_trip_corpus_01_trivial` — iterates `01_trivial` subdirectories.
- `round_trip_corpus_02_reverse_proxy`.
- `round_trip_corpus_03_virtual_hosts`.
- `round_trip_corpus_04_path_matchers`.
- `round_trip_corpus_05_regex_matchers`.
- `round_trip_corpus_06_snippets`.
- `round_trip_corpus_07_imports`.
- `round_trip_corpus_08_env_substitution`.
- `round_trip_corpus_09_tls`.
- `round_trip_corpus_10_multi_site_one_file`.

`core/crates/core/src/caddyfile/normalise.rs`:

- `sort_object_keys_recursively_orders_keys`.
- `strip_trilithon_id_annotations_removes_at_id_keys_starting_with_trilithon`.
- `fold_equivalent_matcher_arrays_collapses_single_element_arrays`.
- `align_automatic_https_disable_redirects_normalises_to_canonical_form`.

### Acceptance command

`cargo test -p trilithon-adapters --test caddyfile_round_trip` (also satisfies the `cargo test -p trilithon-core caddyfile::normalise::tests` target).

### Exit conditions

- All ten round-trip tests pass.
- All four normalisation unit tests pass.
- The harness boots Caddy 2.11.2 and replays the request matrix without skipping fixtures.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.5.

---

## Slice 13.12 [cross-cutting] — `ImportFromCaddyfile` mutation, HTTP endpoints, audit row authoring

### Goal

Wire the import path through the production stack: the `ImportFromCaddyfile` typed mutation persists `source_bytes` and the `LossyWarningSet` in the resulting snapshot's `metadata` blob; `POST /api/v1/imports/caddyfile/preview` returns `{ mutations, warnings }`; `POST /api/v1/imports/caddyfile/apply` returns `{ snapshot_id, warnings }` and writes the `import.caddyfile` audit row.

### Entry conditions

- Slices 13.7, 13.8, and 13.11 complete.
- The Phase 5 snapshot writer accepts a `metadata: serde_json::Value` field.
- The Phase 9 HTTP server hosts authenticated handlers.

### Files to create or modify

- `core/crates/core/src/mutation.rs` — add `ImportFromCaddyfile`.
- `core/crates/cli/src/http/imports.rs` — handlers.
- `core/crates/core/src/audit.rs` — add `AuditEvent::ImportCaddyfile` (whose `Display` returns `"import.caddyfile"`, already in §6.6).

### Signatures and shapes

```rust
// core/crates/core/src/mutation.rs (additions)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImportFromCaddyfile {
    pub source_bytes: Vec<u8>,
    pub source_name: Option<String>,
    pub expected_version: i64,
}

// extends TypedMutation enum
```

```rust
// core/crates/cli/src/http/imports.rs
#[derive(serde::Deserialize)]
pub struct ImportRequestBody {
    pub source: String,             // raw Caddyfile text
    pub source_name: Option<String>,
    pub expected_version: i64,
}

#[derive(serde::Serialize)]
pub struct ImportPreviewResponse {
    pub mutations: Vec<TypedMutation>,
    pub warnings: LossyWarningSet,
}

#[derive(serde::Serialize)]
pub struct ImportApplyResponse {
    pub snapshot_id: SnapshotId,
    pub warnings: LossyWarningSet,
    pub config_version: i64,
}

pub async fn post_preview(
    State(ctx): State<HttpContext>,
    auth: AuthenticatedActor,
    Json(body): Json<ImportRequestBody>,
) -> Result<Json<ImportPreviewResponse>, ApiError>;

pub async fn post_apply(
    State(ctx): State<HttpContext>,
    auth: AuthenticatedActor,
    Json(body): Json<ImportRequestBody>,
) -> Result<Json<ImportApplyResponse>, ApiError>;
```

### Algorithm — `post_preview`

1. Authenticate session.
2. Open span `http.request.received` with `http.method`, `http.path`, `correlation_id`.
3. Run `check_input_size(body.source.as_bytes(), &SizeOptions::default())`. Surface `SizeError` as `413`.
4. Lex / parse / expand / translate. Surface any error as `422` with the structured detail.
5. Return the resulting mutations and warnings.

### Algorithm — `post_apply`

1. Steps 1–4 as `post_preview`.
2. Construct `ImportFromCaddyfile { source_bytes, source_name, expected_version }`.
3. Submit the mutation through the standard mutation pipeline (Phase 7 applier). The applier persists `source_bytes` and `warnings` in the resulting `Snapshot.metadata` blob with intent `"Imported from Caddyfile: <source_name>"`.
4. On success, write `AuditEvent::ImportCaddyfile { source_name, source_bytes_len, warning_count, warning_kinds }` → `import.caddyfile`. The `notes` JSON is `{ source_name, source_bytes_len, warning_count, warning_kinds: [<id>...] }`.
5. Return `200` with `ImportApplyResponse`.

### Tests

Unit tests in `core/crates/core/src/mutation.rs`:

- `import_from_caddyfile_apply_persists_source_bytes_and_warnings_in_snapshot_metadata`.
- `import_from_caddyfile_intent_format`.

Integration tests at `core/crates/cli/tests/imports_http.rs`:

- `preview_returns_mutations_and_warnings_for_trivial_fixture`.
- `preview_rejects_oversize_input_with_413`.
- `preview_rejects_invalid_caddyfile_with_422`.
- `apply_writes_import_caddyfile_audit_row` — assert the row's `kind` is exactly `"import.caddyfile"` (architecture §6.6).
- `apply_persists_source_bytes_in_snapshot_metadata`.
- `apply_returns_409_on_optimistic_conflict`.

### Acceptance command

`cargo test -p trilithon-cli --test imports_http`

### Exit conditions

- All six integration tests pass.
- The audit row's `kind` is `import.caddyfile` verbatim from architecture §6.6.
- `Snapshot.metadata` carries both the original bytes and the warning set.

### Audit kinds emitted

- `import.caddyfile` (architecture §6.6).
- `mutation.submitted` (when the mutation enters the queue).
- `config.applied` / `config.apply-failed` (from the Phase 7 applier).

### Tracing events emitted

- `http.request.received`, `http.request.completed`.
- `apply.started`, `apply.succeeded`, `apply.failed`.

### Cross-references

- PRD T1.5, T1.6, T1.7.
- ADR-0009 (snapshot immutability).
- Architecture §6.5 (`snapshots.metadata`), §6.6.

---

## Slice 13.13 [standard] — Web UI: Import wizard, `LossyWarningList`, `MutationPreviewList`

### Goal

Ship the three-step Import wizard, the reusable `LossyWarningList` (also consumed by Phase 25's Caddyfile export panel), and `MutationPreviewList`. Step 1 accepts paste-or-upload. Step 2 calls preview and renders the mutations and warnings. Step 3 confirms and calls apply.

### Entry conditions

- Slice 13.12 complete; the HTTP endpoints respond.

### Files to create or modify

- `web/src/features/caddyfile-import/ImportWizard.tsx`.
- `web/src/features/caddyfile-import/ImportWizard.test.tsx`.
- `web/src/features/caddyfile-import/MutationPreviewList.tsx`.
- `web/src/features/caddyfile-import/MutationPreviewList.test.tsx`.
- `web/src/features/caddyfile-import/api.ts`.
- `web/src/features/caddyfile-import/types.ts`.
- `web/src/components/LossyWarningList.tsx`.
- `web/src/components/LossyWarningList.test.tsx`.

### Signatures and shapes

```ts
// web/src/features/caddyfile-import/types.ts
export type LossyWarningId =
  | 'unsupported-directive'
  | 'comment-loss'
  | 'ordering-loss'
  | 'snippet-expansion-loss'
  | 'env-substitution-empty'
  | 'tls-dns-provider-unavailable'
  | 'placeholder-passthrough'
  | 'capability-degraded';

export interface LossyWarning {
  readonly id: LossyWarningId;
  readonly message: string;
  readonly span?: { line: number; column: number };
  readonly details?: Readonly<Record<string, unknown>>;
}

export interface ImportPreviewResponse {
  readonly mutations: readonly TypedMutation[];
  readonly warnings: { readonly warnings: readonly LossyWarning[] };
}

export interface ImportApplyResponse {
  readonly snapshot_id: string;
  readonly warnings: { readonly warnings: readonly LossyWarning[] };
  readonly config_version: number;
}
```

```tsx
// web/src/components/LossyWarningList.tsx
export interface LossyWarningListProps {
  readonly warnings: readonly LossyWarning[];
}
export function LossyWarningList(props: LossyWarningListProps): JSX.Element;
```

```tsx
// web/src/features/caddyfile-import/ImportWizard.tsx
export function ImportWizard(): JSX.Element;
```

```tsx
// web/src/features/caddyfile-import/MutationPreviewList.tsx
export interface MutationPreviewListProps {
  readonly mutations: readonly TypedMutation[];
}
export function MutationPreviewList(props: MutationPreviewListProps): JSX.Element;
```

### Algorithm — `ImportWizard` state machine

States: `Source`, `Previewing`, `Reviewing`, `Applying`, `Done`, `Error`.

1. `Source`: user pastes text or uploads a file. The "Next" button is disabled while the source is empty.
2. On "Next", transition `Previewing`; call `runPreview`.
3. On `200`, transition `Reviewing` with mutations and warnings. Render `MutationPreviewList` and `LossyWarningList` side by side.
4. On `4xx`, transition `Error`.
5. On "Apply" in `Reviewing`, transition `Applying`; call `runApply`.
6. On `200`, transition `Done`; show the resulting snapshot id.

### Tests

Vitest tests at `web/src/features/caddyfile-import/ImportWizard.test.tsx`:

- `wizard_step_1_disables_next_when_source_empty`.
- `wizard_step_2_renders_mutations_and_warnings_from_preview`.
- `wizard_step_3_calls_apply_and_renders_snapshot_id`.
- `wizard_renders_413_error_for_oversize_input`.
- `wizard_renders_422_error_with_inline_message`.

Vitest tests at `web/src/components/LossyWarningList.test.tsx`:

- `lossy_warning_list_renders_id_message_and_span_for_each_warning`.
- `lossy_warning_list_passes_axe_with_zero_serious_violations`.

Vitest tests at `web/src/features/caddyfile-import/MutationPreviewList.test.tsx`:

- `mutation_preview_list_renders_one_row_per_mutation`.

### Acceptance command

`pnpm vitest run web/src/features/caddyfile-import web/src/components/LossyWarningList.test.tsx`

### Exit conditions

- All eight Vitest tests pass.
- `pnpm typecheck` passes.
- `pnpm lint` passes.
- The `LossyWarningList` is exported from `web/src/components/` for Phase 25 reuse.

### Audit kinds emitted

None directly from the web UI.

### Tracing events emitted

None directly.

### Cross-references

- PRD T1.5.
- Phase 25 (Caddyfile export, reuses `LossyWarningList`).

---

## Phase 13 exit checklist

- [ ] Every slice from 13.1 through 13.13 has shipped and its acceptance command passes.
- [ ] `just check` passes locally and in continuous integration.
- [ ] For every non-pathological fixture, the round-trip equivalence harness reports byte-identical normalised JSON and matching request-matrix responses against `caddy adapt`.
- [ ] Every parse that loses information emits at least one `LossyWarning` of the appropriate variant; the corpus covers every variant.
- [ ] All five size bounds reject pathological fixtures within two seconds on reference hardware without resident memory exceeding 256 MiB.
- [ ] The `ImportFromCaddyfile` mutation attaches the original bytes and the warning set to the resulting snapshot.
- [ ] The `import.caddyfile` audit kind appears verbatim in architecture §6.6 and is asserted by an integration test.
- [ ] The `LossyWarningList` component is reusable from Phase 25.

## Open questions

1. The pathological-rejection memory budget of 256 MiB is sampled with `getrusage` on Linux and macOS. Whether a comparable Windows measurement (relevant once the desktop wrapper ships in V1.1) requires a different sampler is unresolved and is filed here.
2. The set of "global" Caddyfile directives the parser recognises at top level (slice 13.3 step 3.2) is taken from Caddy 2.11.2. Future Caddy releases may add globals; the version pin file enforces a controlled bump.
3. Whether deterministic ULID generation from canonicalised addresses (slice 13.7 step 1) is preferable to fresh ULIDs is unresolved. Deterministic ids ease golden testing but break the convention that snapshot ids are content-addressed and never function-of-input. The fixtures in this phase use a fresh ULID with golden assertions ignoring the id field.

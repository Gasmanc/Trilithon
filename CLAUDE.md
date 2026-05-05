# CLAUDE.md

Guidance for Claude Code when working in **.**.

<!-- new-project:managed:begin -->
*This file is partially managed by `new-project`. Sections delimited by
`<!-- new-project:*:begin/end -->` are regenerated on `new-project --resync`.
Hand-written content outside those delimiters is preserved.*
<!-- new-project:managed:end -->

## Project overview

_(One-line overview goes here.)_

## The gate

`just check` is the gate. Run it before declaring work complete. CI also
runs it. If it fails, fix the cause; do not bypass the linter.

## Behavioral constitution

<!-- new-project:behavior:begin -->
- **Reuse over new code.** Grep before writing. Prefer existing utilities,
  components, and patterns. New abstractions need a real reason.
- **No suppressions without justification.** If you must `#[allow]`,
  `eslint-disable`, `# noqa`, etc., leave an inline comment with a tracked id
  (`zd:<id> expires:<YYYY-MM-DD> reason:<short>`).
- **No TODOs in committed code.** Either implement, or open a tracked issue
  and reference it.
- **No mocks/stubs in non-test paths.** Mocks and stubs live in test files
  and test directories only.
- **No `unwrap()`/`!`/non-null assertions in production code.** Tests are
  fine. In production, handle the error path or document why a panic is
  correct.
- **Limit over-abstraction.** Three uses before extracting a helper.
  Premature generalisation costs more than copy-paste.
- **Run the gate before done.** `just check` must pass.
<!-- new-project:behavior:end -->



<!-- new-project:lang:rust:begin -->
## Rust conventions (workspace, three-layer)

**Layout**

- `crates/core/` — pure logic. **No I/O, no async runtime, no FFI.** Manifest must not declare those dependencies.
- `crates/adapters/` — wraps the outside world (db, http, fs, env, time). Depends on `core`.
- `crates/cli/` — the binary. Depends on `core` + `adapters`. Wires arguments + signals + tracing.

The architectural rule is enforced by manifest dependencies. If you need a cross-layer dep, **stop and ask** — it usually means the design needs adjustment.

**Stack**

- Edition 2024, MSRV 1.80
- Async: Tokio
- Errors: `thiserror` in `core`/`adapters`, `anyhow` in `cli`
- Logging: `tracing` + `tracing-subscriber`
- CLI: `clap` derive
- Serialisation: `serde` + `serde_json`

**Style**

- `rustfmt` defaults, no overrides.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` must pass.
- Prefer `?` over `match` for error propagation.
- Prefer iterators and combinators over index loops.
- Concrete error types in `core`/`adapters`; `anyhow::Result` only in `cli` and tests.
- No `unwrap()`/`expect()` outside tests.
- No `unsafe` (`unsafe_code = forbid` in `core`/`adapters`).

**Testing**

- Unit tests inline (`#[cfg(test)] mod tests`); integration tests in `tests/`.
- `insta` for output > a few lines; `proptest` for invariants.
- Every public item gets a doc comment.

**Workflow**

- `just check-rust` is the gate.
- `cargo deny check` (via `just deny-rust`) before bumping deps.
<!-- new-project:lang:nd -->

<!-- new-project:lang:typescript:begin -->
## TypeScript / React conventions (frontend)

**Stack**

- TypeScript ~5.6 (strict + `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`)
- React 19
- Vite 5
- Tailwind 3
- ESLint 9 (flat config) with `typescript-eslint` strict-type-checked
- Prettier
- Vitest for tests
- Package manager: pnpm

**Style**

- Run `pnpm typecheck && pnpm lint && pnpm format:check && pnpm test --run` (all wrapped by `just check-typescript`).
- No `any`. Use `unknown` and narrow.
- No non-null assertions (`!`). Use type guards or runtime checks.
- No `@ts-ignore` (use `@ts-expect-error` with a 10+ char description).
- No floating promises (`@typescript-eslint/no-floating-promises`).
- No `console.log` in production code (`console.warn`/`console.error` allowed).
- No TODO/FIXME/XXX/HACK in committed code (`no-warning-comments` enforces this).
- Mocks/`vitest`/`@testing-library` are only importable from `*.test.ts(x)`/`*.spec.ts(x)`.

**Components**

- Function components only. Hooks over class lifecycle.
- Co-locate styles via Tailwind classes. Avoid CSS-in-JS unless there's a real reason.
- One component per file when over ~50 lines; otherwise grouping is fine.

**Testing**

- Vitest. Place tests next to the file (`Foo.tsx` → `Foo.test.tsx`) or under `__tests__/`.
- React Testing Library for component tests. No `enzyme`.
<!-- new-project:lang:nd -->

<!-- new-project:lang:swift:begin -->
## Swift conventions (xcode-app, SwiftUI)

**Layout**

- Xcode project (`.xcodeproj`) generated from `project.yml` via `xcodegen`.
- `Sources/{{SWIFT_NAME}}/` — app code (SwiftUI views, view models, etc.).
- `Tests/{{SWIFT_NAME}}Tests/` — XCTest tests.

To regenerate the project after editing `project.yml`: `xcodegen generate`.

**Stack**

- Swift 5.9+
- SwiftUI
- Linter: SwiftLint (strict, also wired as a Run Script phase)
- Formatter: SwiftFormat
- Tests: XCTest
- Project generator: xcodegen

**Style**

- `xcodegen generate && swiftlint lint --strict && swiftformat --lint .` is the gate (`just check-swift`).
- No `try!`, no `as!`, no `!` (force-unwrap) — SwiftLint enforces.
- No `fatalError(...)` without an explanatory message.
- No `Mock*`/`Stub*`/`Fake*` types outside `*Tests.swift` or `Tests/`.
- No `TODO`/`FIXME`/`XXX`/`HACK` in committed code.
- Prefer `@Observable` (Swift 5.9 macro) over `ObservableObject` for new view models.
- Keep views small. Extract subviews when a view exceeds ~50 lines.

**Workflow**

- `just open-app` opens the project in Xcode.
- Treat `project.yml` as the source of truth — never hand-edit `.xcodeproj`.
<!-- new-project:lang:nd -->

## Architecture

<!-- new-project:architecture:begin -->
This project uses three-layer separation enforced by package/crate/module boundaries:

- `core/` — pure logic, no I/O. Depends on nothing.
- `adapters/` — wraps the outside world (db, http, fs, env). Depends on core.
- `entry/` (or `cli/`, `server/`, `app/`) — main(), CLI, server. Depends on core + adapters.

The architectural rule lives in the *absence* of cross-layer dependencies in
the manifest. The compiler/tsc/import-linter does the rest. If you need a
new cross-layer dependency, **stop and ask** — it usually means a design
issue, not a graph issue.
<!-- new-project:architecture:end -->

## Domain notes

*Project-specific terminology, invariants, and frozen zones go here. Keep
this section. Update it as the project grows.*

_(Project-specific terminology and invariants go here.)_

## Commits and branches

- Conventional Commits: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
- Scope optional but encouraged: `feat(parser): handle trailing commas`.
- Branch names: `feat/short-description`, `fix/short-description`.
- Keep commits focused. Several small commits beat one sprawling one.

## Knowledge Store

Before implementing any new pattern in this codebase, search `docs/solutions/` using
the learnings-researcher agent. This is mandatory, not optional.

After completing a phase, run `/compound <phase-id>` if `/review-remediate` flagged
compound candidates.

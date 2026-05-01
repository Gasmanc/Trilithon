# ADR-0004: Adopt React 19, TypeScript strict, Tailwind 3, and Vite 5 for the frontend

## Status

Accepted — 2026-04-30.

## Context

Trilithon's primary user interface is a web application served on
loopback by the Trilithon daemon (T1.13). The binding prompt (section
2, item 5) fixes the stack: React 19, TypeScript ~5.6 (strict), Tailwind
3, Vite 5, Vitest. The web UI is delivered first; the Tauri desktop
wrap is V1.1 (ADR-0005).

Forces:

1. **Local-first delivery.** The frontend runs in the user's browser,
   served by the daemon over loopback. There is no third-party hosting
   tier, no edge runtime, and no server-side rendering requirement.
   Static assets compiled by Vite are sufficient.
2. **Power-user surface.** The dual-pane editor (T1.12), the access
   log viewer (T2.5), and the diff/preflight UI all involve dense
   interactive components with non-trivial state. React's component
   model and ecosystem (code editors, virtualised lists) shorten the
   path from spec to working UI.
3. **Strict types as a defect filter.** The typed mutation API
   (T1.6, ADR-0008) is consumed by the frontend. TypeScript with
   `strict`, `noUncheckedIndexedAccess`, and `exactOptionalPropertyTypes`
   prevents an entire class of mismatch between the wire schema and
   the rendered form.
4. **Build-time speed.** Vite's dev server uses native ES modules and
   esbuild for transformation. The feedback loop on a 50-component
   project is sub-second, which matters because the frontend is the
   surface most contributors will iterate on.
5. **No CSS sprawl.** Tailwind's utility classes co-locate styling
   with markup, eliminate global stylesheet conflicts, and produce
   a tree-shaken final bundle. CSS-in-JS adds runtime cost the
   project does not need.
6. **Tauri compatibility.** Vite's static output is the input to a
   Tauri 2.x build (ADR-0005). Choosing a non-Vite bundler now would
   require a migration when V1.1 lands.

## Decision

Trilithon's web UI SHALL be implemented in React 19 function components
with hooks. Class components SHALL NOT be introduced.

The TypeScript compiler SHALL be configured with `strict: true`,
`noUncheckedIndexedAccess: true`, and `exactOptionalPropertyTypes:
true`. The use of `any` SHALL be forbidden (`@typescript-eslint/no-explicit-any`).
Non-null assertions (`!`) SHALL NOT appear in production code paths
(constraint 7 of the binding prompt; the project `CLAUDE.md` extends
the rule to TypeScript). `@ts-ignore` SHALL NOT appear; `@ts-expect-error`
MAY appear with a comment of at least ten characters explaining the
expectation.

Styling SHALL use Tailwind 3 utility classes. CSS-in-JS libraries
(`styled-components`, `emotion`, `stitches`) SHALL NOT be introduced
without a follow-up ADR. Component-scoped CSS modules MAY be used
where Tailwind classes would harm readability.

The build tool SHALL be Vite 5. The dev server, the production build,
and the Tauri-compatible static output SHALL all flow through Vite's
configuration.

The test runner SHALL be Vitest. Component tests SHALL use React Testing
Library. Test files (`*.test.ts(x)`, `*.spec.ts(x)`) SHALL be the only
files permitted to import `vitest` or `@testing-library/*` (constraint
8 of the binding prompt).

The package manager SHALL be pnpm. Lockfile churn SHALL be reviewed in
pull requests.

`just check-typescript` SHALL run, in this order: `pnpm typecheck`,
`pnpm lint`, `pnpm format:check`, `pnpm test --run`. Any failure SHALL
block merging.

## Consequences

**Positive.**

- The frontend stack is well-understood, well-documented, and matches
  the project owner's stated preferences and existing scaffolding
  (`web/package.json`, `web/vite.config.ts`, `web/tsconfig.json`).
- The strict TypeScript configuration catches schema-shape mismatches
  between the typed mutation API and the UI at compile time, reducing
  the class of bugs that a runtime validator would otherwise have to
  catch.
- Tailwind's tree-shaking keeps the production bundle small. The
  loopback delivery target benefits from a small bundle even though
  network cost is near zero, because parse and execute time on
  underpowered hardware (a Raspberry Pi running the daemon) still
  matters.

**Negative.**

- A user with a strong preference for Vue, Svelte, or SolidJS cannot
  contribute frontend code without a stack switch. The user is the
  product owner, and the choice reflects that.
- React 19's recent release means some libraries lag in compatibility.
  Trilithon SHALL track upstream React releases and SHALL NOT pin to
  pre-stable React without an ADR.
- Tailwind 3's class-name verbosity in markup is a real readability
  cost in dense components. The project accepts this cost in exchange
  for the absence of stylesheet conflicts.

**Neutral.**

- The frontend has no state-management library by default. React's
  built-in primitives (`useState`, `useReducer`, `useContext`) cover
  V1 needs. If pressure from real components justifies it, a
  follow-up ADR may introduce a library. Speculative adoption is
  forbidden by the three-uses rule.
- The ESLint configuration is `eslint.config.js` (flat config) with
  `typescript-eslint` strict-type-checked rules. The configuration is
  versioned under `web/eslint.config.js`.

## Alternatives considered

**Vue 3 with TypeScript and Vite.** Equivalent build story, smaller
mental model in some respects, single-file components keep template
and logic together. Rejected because the project owner has chosen
React, the existing scaffold is React, and the Tauri ecosystem and
component libraries (code editors, table virtualisers) skew toward
React.

**SvelteKit.** Excellent ergonomics, smaller runtime. Rejected because
SvelteKit's value proposition is server-side rendering and edge
runtime targets, neither of which Trilithon needs (the daemon serves
static assets on loopback). Choosing SvelteKit would pay a runtime
complexity cost for unused features.

**Next.js.** A React framework with file-system routing and a
production server. Rejected because Trilithon's daemon is the
production server; introducing a Node.js runtime alongside the Rust
daemon doubles the deployment surface and contradicts ADR-0010's
two-container model.

**Plain React with webpack.** Stable and battle-tested. Rejected
because Vite's dev-server feedback is materially faster on the
project's component count, because Vite's static output is what
Tauri 2.x consumes, and because webpack's configuration surface
adds maintenance cost without compensating value.

**Server-rendered HTML with HTMX.** Rejected because the dual-pane
editor (T1.12), the diff preview, and the access log viewer (T2.5)
require client-side state that does not fit a request-per-interaction
model.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#2-non-negotiable-constraints`,
  items 5, 7, 8; section 4 features T1.12, T1.13; section 5 features
  T2.5.
- Project `CLAUDE.md` TypeScript / React conventions section.
- ADR-0005 (Web UI first, Tauri desktop deferred to V1.1).
- ADR-0008 (Bounded typed tool gateway for language models).

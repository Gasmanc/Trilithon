# Phase 11 — Web UI shell, auth, and route CRUD

Source of truth: [`../phases/phased-plan.md#phase-11--web-ui-shell-auth-and-route-crud`](../phases/phased-plan.md#phase-11--web-ui-shell-auth-and-route-crud).

> **Path-form note.** Web frontend paths are anchored at `web/`. Per-feature directories: `web/src/features/auth/`, `web/src/features/dashboard/`, `web/src/features/routes/`, `web/src/components/diff/`, `web/src/lib/api/`, `web/src/lib/auth/`. Vitest tests live next to the implementation as `<Component>.test.tsx`. See [`README.md`](README.md) "Path conventions".

## Pre-flight checklist

- [ ] Phase 9 complete (HTTP API operational).
- [ ] Phase 10 complete (secret-marked fields in routes are handled correctly end-to-end).

## Tasks

### Frontend / shell

- [ ] **Replace the Vite skeleton with the Trilithon shell.**
  - Acceptance: `web/src/` MUST host a React 19 application with a router covering `/login`, `/`, `/routes`, `/routes/:id`, `/audit`, `/snapshots`, an authenticated layout, an unauthenticated layout, and a global error boundary. Component signatures MUST be:

    ```tsx
    export function App(): JSX.Element;
    export function LoginPage(): JSX.Element;
    export function BootstrapPage(): JSX.Element;
    export function DashboardPage(): JSX.Element;
    export function RoutesIndex(): JSX.Element;
    export function RouteDetail(props: { routeId: RouteId }): JSX.Element;
    export function CreateRouteForm(props: { onSubmit: (m: CreateRouteMutation) => Promise<void> }): JSX.Element;
    export function UpdateRouteForm(props: {
      initial:  Route;
      onSubmit: (m: UpdateRouteMutation) => Promise<void>;
    }): JSX.Element;
    export function DeleteRouteButton(props: { routeId: RouteId; expectedVersion: number }): JSX.Element;
    ```

    Files: `web/src/App.tsx`, `web/src/features/auth/LoginPage.tsx`, `web/src/features/auth/BootstrapPage.tsx`, `web/src/features/dashboard/DashboardPage.tsx`, `web/src/features/routes/RoutesIndex.tsx`, `web/src/features/routes/RouteDetail.tsx`, `web/src/features/routes/CreateRouteForm.tsx`, `web/src/features/routes/UpdateRouteForm.tsx`, `web/src/features/routes/DeleteRouteButton.tsx`.
  - Done when: `pnpm dev` renders the shell and `pnpm test --run` passes the router tests.
  - Feature: T1.13.
- [ ] **Generate a typed API client from the OpenAPI document.**
  - Acceptance: `web/src/api/` MUST contain a typed client generated from the Phase 9 OpenAPI document; `fetch` MUST use `credentials: "include"` and surface typed errors.
  - Done when: a script `pnpm generate:api` regenerates the client and the test suite asserts no `any` types.
  - Feature: T1.13.

### Frontend / authentication

- [ ] **Implement the login page with bootstrap step-up.**
  - Acceptance: The login page MUST implement bootstrap-flow step-up: first login MUST surface "Change your password" before any other UI is reachable.
  - Done when: a Vitest test exercises the bootstrap branch and asserts the redirect.
  - Feature: T1.14.

### Frontend / dashboard

- [ ] **Implement the dashboard skeleton.**
  - Acceptance: The dashboard MUST render the capability summary, a drift-state banner (zero, one, or many drifts), and an apply-in-flight banner.
  - Done when: Vitest tests cover all three states.
  - Feature: T1.13.

### Frontend / routes

- [ ] **Implement the routes index page.**
  - Acceptance: The routes index MUST be paginated and MUST list hostname, primary upstream, status (enabled, disabled, errored), TLS badge placeholder, and upstream-health badge placeholder.
  - Done when: a Vitest test renders 25 routes across two pages.
  - Feature: T1.8.
- [ ] **Implement route create.**
  - Acceptance: The create form MUST accept hostname, upstream targets, optional path matchers, and optional headers; client-side validation MUST gate submission.
  - Done when: a Vitest test asserts the form submits a `CreateRoute` mutation only when valid.
  - Feature: T1.8.
- [ ] **Implement route read.**
  - Acceptance: The detail page MUST show current desired state plus a small history strip of recent snapshots affecting the route.
  - Done when: a Vitest test renders a fixture and asserts the history strip.
  - Feature: T1.8.
- [ ] **Implement route update with diff preview.**
  - Acceptance: Edits MUST produce a diff preview before submission; the Apply button MUST be disabled until the form is valid.
  - Done when: a Vitest test asserts the disabled state and the rendered diff.
  - Feature: T1.8.
- [ ] **Implement route delete with confirmation.**
  - Acceptance: Deletion MUST require an explicit typed confirmation and MUST produce a `DeleteRoute` mutation and an audit row.
  - Done when: a Vitest test asserts the confirmation gate.
  - Feature: T1.8.

### Frontend / diff and validation

- [ ] **Implement the diff preview component.**
  - Path: `web/src/components/diff/DiffPreview.tsx` and `web/src/components/diff/DiffPreview.test.tsx`.
  - Acceptance: The diff component MUST colour-code additions, removals, and modifications, render secret-marked fields as `***` plus the ciphertext-derived hash prefix, and never render plaintext secrets. The TypeScript signatures MUST be:

    ```tsx
    export function DiffPreview(props: DiffPreviewProps): JSX.Element;
    export type DiffPreviewProps = {
      before:        DesiredState;
      after:         DesiredState;
      redactSecrets?: boolean;          // default true
    };
    export type DiffEntry =
      | { kind: 'added';    path: JsonPointer; after:  unknown }
      | { kind: 'removed';  path: JsonPointer; before: unknown }
      | { kind: 'modified'; path: JsonPointer; before: unknown; after: unknown };
    ```
  - Done when: Vitest tests cover all three diff classes and the redaction invariant.
  - Feature: T1.8 (mitigates H10).
- [ ] **TypeScript types mirroring the Phase 4 Rust model.**
  - Path: `web/src/lib/api/types.ts`.
  - Acceptance: TypeScript types MUST be defined for `Route`, `Upstream`, `MutationEnvelope`, `LoginRequest`, `BootstrapResponse`. These mirror the Phase 4 Rust types via `serde_json` plus `ts-rs` (or hand-written equivalents kept in sync with the OpenAPI document). Every mutation request envelope MUST include `expected_version: number`.
  - Done when: a Vitest test asserts the envelope shape against a fixture response.
  - Feature: T1.8.
- [ ] **Implement field-level validation feedback.**
  - Acceptance: The forms MUST produce field-level errors for invalid hostnames, unreachable port numbers, and malformed regular-expression matchers. The validators are:
    - **Hostname** (RFC 952 + RFC 1123): regex `^(?=.{1,253}$)([a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)(\.([a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?))*$`.
    - **Port**: `Number.isInteger(p) && p >= 1 && p <= 65535`.
    - **Regex matcher**: compile via `new RegExp(...)` inside a `try/catch`; surface the thrown `SyntaxError.message`.

    All field validation on the client mirrors the server's `POST /api/v1/desired-state/validate` endpoint; the server is authoritative.
  - Done when: Vitest tests cover each error class.
  - Feature: T1.8.

### Frontend / styling

- [ ] **Use Tailwind classes; avoid CSS-in-JS.**
  - Acceptance: Component styling MUST be Tailwind-only; CSS-in-JS is forbidden absent an explicit project decision.
  - Done when: a lint rule fails the build on any `styled-components` or `emotion` import.
  - Feature: T1.13.

### Tests

- [ ] **Diff preview renders adds, removes, and mods correctly.**
  - Acceptance: The Vitest test corpus MUST exercise every diff class.
  - Done when: `pnpm test --run` passes.
  - Feature: T1.8.
- [ ] **Apply is disabled until the form is valid.**
  - Acceptance: A Vitest test MUST assert the Apply button is disabled while validation is failing.
  - Done when: the test passes.
  - Feature: T1.8.
- [ ] **Secret-marked fields never render plaintext.**
  - Acceptance: A Vitest test MUST exercise every read view that displays a secret-marked field and assert it renders `***` plus the hash prefix.
  - Done when: the test passes.
  - Feature: T1.8 (mitigates H10).

## Cross-references

- ADR-0004 (React, TypeScript, Tailwind, Vite frontend stack).
- ADR-0011 (loopback-only by default).
- ADR-0014 (secrets vault — UI never renders plaintext).
- PRD T1.8 (route create / read / update / delete), T1.13 (web UI delivery).
- Architecture: "Web UI — shell," "Diff preview," "Validation pipeline."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration (Rust gate plus `pnpm typecheck && pnpm lint && pnpm format:check && pnpm test --run`).
- [ ] A user who has never touched a configuration file can install Trilithon, open `http://127.0.0.1:7878`, log in with bootstrap credentials, change their password, and create their first route, satisfying T1.13.
- [ ] A newly created route serves traffic within five seconds of approval, given a healthy Caddy.
- [ ] A deleted route stops serving traffic within five seconds.
- [ ] An update is atomic; there is no observable window where the route is half-updated.
- [ ] Apply is disabled while validation is failing.
- [ ] No plaintext secret is rendered in any read view.

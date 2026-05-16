# Phase 11 — Web UI shell, auth, and route CRUD — Implementation Slices

> Phase reference: [../phases/phase-11-web-ui-shell.md](../phases/phase-11-web-ui-shell.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [../phases/phase-11-web-ui-shell.md](../phases/phase-11-web-ui-shell.md).
- Architecture §4.5 (UI shell), §4.6 (Route management), §6.6 (audit kinds), §11 (security posture), §12 (observability).
- Phase 9 OpenAPI document at `/api/v1/openapi.json`.
- ADRs: ADR-0004 (React, TypeScript, Tailwind, Vite frontend stack), ADR-0011 (loopback-only by default), ADR-0014 (secrets vault — UI never renders plaintext).

## Slice plan summary

| # | Title | Primary files | Effort (h) | Depends on |
|---|-------|---------------|-----------:|-----------|
| 11.1 | Replace Vite skeleton with `App` shell, router, layouts, error boundary | `web/src/App.tsx`, `web/src/main.tsx`, `web/src/lib/layout/*.tsx` | 6 | Phase 9, Phase 10 |
| 11.2 | OpenAPI-generated typed API client (`pnpm generate:api`) | `web/src/lib/api/generated/*`, `web/scripts/generate-api.mjs`, `web/package.json` | 5 | 11.1 |
| 11.3 | Mutation envelope and shared API types (`Route`, `Upstream`, `MutationEnvelope`, …) | `web/src/lib/api/types.ts`, `web/src/lib/api/client.ts` | 4 | 11.2 |
| 11.4 | Login page, bootstrap step-up, change-password page | `web/src/features/auth/LoginPage.tsx`, `web/src/features/auth/BootstrapPage.tsx`, `web/src/features/auth/ChangePasswordPage.tsx` | 6 | 11.3 |
| 11.5 | Dashboard with capability summary, drift banner, apply-in-flight banner | `web/src/features/dashboard/DashboardPage.tsx` | 5 | 11.3 |
| 11.6 | Routes index with pagination | `web/src/features/routes/RoutesIndex.tsx` | 4 | 11.3 |
| 11.7 | Route detail, history strip, delete with confirmation | `web/src/features/routes/RouteDetail.tsx`, `web/src/features/routes/DeleteRouteButton.tsx` | 5 | 11.6 |
| 11.8 | Route create and update forms with field-level validation | `web/src/features/routes/CreateRouteForm.tsx`, `web/src/features/routes/UpdateRouteForm.tsx`, `web/src/lib/validation/*.ts` | 7 | 11.6 |
| 11.9 | `DiffPreview` component with secret redaction enforcement | `web/src/components/diff/DiffPreview.tsx`, `web/src/components/diff/DiffPreview.test.tsx` | 5 | 11.8 |

---

## Slice 11.1 [cross-cutting] — Replace Vite skeleton with `App` shell, router, layouts, error boundary

### Goal

Replace the scaffolded `web/src/App.tsx` with a Trilithon shell that mounts a `react-router-dom` v6 router covering `/login`, `/`, `/routes`, `/routes/:id`, `/audit`, `/snapshots`, plus a global error boundary, a `RequireAuth` guard, and two layouts (authenticated and unauthenticated). Tailwind utility classes are the only styling.

### Entry conditions

- Phase 9 done; the daemon serves the API.
- Phase 10 done; secrets are encrypted end-to-end.
- `web/package.json` already lists React 19, TypeScript ~5.6, Tailwind 3, Vite 5 (the project scaffold).

### Files to create or modify

- `web/src/App.tsx` — top-level component.
- `web/src/main.tsx` — entry; mounts `<App />` with `<BrowserRouter>`.
- `web/src/lib/layout/AuthenticatedLayout.tsx` — shell with sidebar and content area.
- `web/src/lib/layout/UnauthenticatedLayout.tsx` — login layout.
- `web/src/lib/layout/RequireAuth.tsx` — guard.
- `web/src/lib/layout/ErrorBoundary.tsx` — global error boundary.
- `web/package.json` — add `react-router-dom@^6.x`.

### Signatures and shapes

```tsx
// web/src/App.tsx
import type { JSX } from 'react';

export function App(): JSX.Element;

// web/src/lib/layout/RequireAuth.tsx
export type RequireAuthProps = {
  children: JSX.Element;
};
export function RequireAuth(props: RequireAuthProps): JSX.Element;

// web/src/lib/layout/ErrorBoundary.tsx
export type ErrorBoundaryProps = { children: JSX.Element };
export type ErrorBoundaryState = { error: Error | null };
export class ErrorBoundary
  extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
  /* render fallback UI on caught errors */
}

// Route declarations
export const ROUTE_PATHS = {
  login:           '/login',
  changePassword:  '/change-password',
  dashboard:       '/',
  routesIndex:     '/routes',
  routeDetail:     '/routes/:id',
  audit:           '/audit',
  snapshots:       '/snapshots',
} as const;
```

### Algorithm

1. `App` renders `<ErrorBoundary><Routes>...</Routes></ErrorBoundary>`.
2. The route table maps:
   - `/login` → `<UnauthenticatedLayout><LoginPage /></UnauthenticatedLayout>`.
   - `/change-password` → `<UnauthenticatedLayout><ChangePasswordPage /></UnauthenticatedLayout>`.
   - All others → `<RequireAuth><AuthenticatedLayout><Outlet /></AuthenticatedLayout></RequireAuth>`.
3. `RequireAuth` calls `useAuthSession()` (slice 11.3 ships this hook). If the session is loading, render a spinner. If unauthenticated, redirect to `/login` via `<Navigate to="/login" replace />`. If authenticated and `mustChangePw`, redirect to `/change-password`.
4. `AuthenticatedLayout` renders a left rail with links to Dashboard, Routes, Audit, Snapshots and a top bar carrying the current user and a logout button.

### Tests

- `web/src/App.test.tsx` — `render(<MemoryRouter><App /></MemoryRouter>)`; assert the router mounts.
- `web/src/lib/layout/RequireAuth.test.tsx` — three cases: loading (spinner), unauthenticated (redirect to `/login`), authenticated (renders children).
- `web/src/lib/layout/ErrorBoundary.test.tsx` — render a child that throws; assert the fallback UI appears with the error message.

### Acceptance command

`pnpm typecheck && pnpm lint && pnpm test --run web/src/App.test.tsx web/src/lib/layout`

### Exit conditions

- `pnpm dev` MUST render the shell.
- `pnpm typecheck` MUST pass with `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`.
- No CSS-in-JS imports MUST appear (lint-enforced in slice 11.8 onwards; the rule lands here).

### Audit kinds emitted

None directly. The frontend triggers audit events via API calls.

### Tracing events emitted

None directly. The frontend's `X-Correlation-Id` headers are surfaced to the daemon, which emits `http.request.received`.

### Cross-references

- ADR-0004.
- PRD T1.13.
- Architecture §4.5.

---

## Slice 11.2 [cross-cutting] — OpenAPI-generated typed API client

### Goal

Generate a typed API client from the Phase 9 OpenAPI document. The generator runs via `pnpm generate:api`; the output lives under `web/src/lib/api/generated/`. The client uses `fetch` with `credentials: 'include'` and surfaces typed errors. No `any` MAY appear in the generated code.

### Entry conditions

- Slice 11.1 done.
- Phase 9 publishes `/api/v1/openapi.json`.

### Files to create or modify

- `web/scripts/generate-api.mjs` — Node script invoking the generator.
- `web/package.json` — add `"generate:api": "node scripts/generate-api.mjs"`; add `openapi-typescript` and `openapi-fetch` as devDependencies.
- `web/src/lib/api/generated/schema.ts` — generator output.
- `web/src/lib/api/generated/client.ts` — re-export of `createClient`.
- `.gitignore` — exclude the generated file from `lint:format` if needed.

### Signatures and shapes

```ts
// web/src/lib/api/generated/schema.ts (auto-generated; signature shown for orientation)
export interface paths {
  '/api/v1/health':  { get: { /* ... */ } };
  '/api/v1/auth/login': { post: { requestBody: { content: { 'application/json': components['schemas']['LoginRequest'] } } } };
  /* every endpoint from Phase 9 slice 9.11 */
}

export interface components {
  schemas: {
    LoginRequest:        { username: string; password: string };
    LoginResponse:       { user_id: string; role: 'owner'|'operator'|'reader'; must_change_pw: boolean; config_version: number };
    MutationEnvelope:    { expected_version: number; body: unknown };
    MutationResponse:    { snapshot_id: string; config_version: number };
    SnapshotSummary:     { id: string; parent_id: string | null; config_version: number; created_at: number; actor_kind: string; actor_id: string; intent: string };
    AuditRow:            { /* per slice 9.9 */ };
    DriftCurrent:        { /* per slice 9.10 */ };
    Capabilities:        { /* per slice 9.11 */ };
    ApiError:            { code: string; detail?: string };
    /* ... */
  };
}
```

```ts
// web/src/lib/api/client.ts
import createClient from 'openapi-fetch';
import type { paths } from './generated/schema';

export const apiClient = createClient<paths>({
  baseUrl: '/',
  credentials: 'include',
});
```

### Algorithm

1. The generator script fetches the OpenAPI document either from a local URL passed via `TRILITHON_OPENAPI_URL` or from a checked-in fallback at `web/src/lib/api/openapi.json`.
2. It runs `npx openapi-typescript <input> --output web/src/lib/api/generated/schema.ts`.
3. CI runs `pnpm generate:api && git diff --exit-code web/src/lib/api/generated/` to assert the committed file is in sync with the daemon's published schema.
4. The runtime client (`apiClient`) wraps `openapi-fetch::createClient`, defaulting `credentials: 'include'` so session cookies travel.

### Tests

- `web/src/lib/api/client.test.ts` — `vi.fn()` mocks `fetch`; assert `apiClient.GET('/api/v1/health')` issues `credentials: 'include'`.
- `web/src/lib/api/types-no-any.test.ts` — a synthetic type test using `expect-type` asserts every leaf in `components.schemas` is non-`any`.

### Acceptance command

`pnpm generate:api && pnpm typecheck && pnpm test --run web/src/lib/api/`

### Exit conditions

- `pnpm generate:api` MUST regenerate `schema.ts` from a published OpenAPI document.
- The generated file MUST contain no `any` types.
- The runtime client MUST set `credentials: 'include'`.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.13.
- ADR-0004.

---

## Slice 11.3 [cross-cutting] — Mutation envelope and shared API types

### Goal

Hand-write the Trilithon-specific helper types that the generator does not automatically synthesise: `Route`, `Upstream`, the typed `MutationEnvelope<T>`, and a `useAuthSession` hook backed by the Phase 9 endpoints. Every mutation request envelope MUST include `expected_version: number`.

### Entry conditions

- Slice 11.2 done.

### Files to create or modify

- `web/src/lib/api/types.ts` — shared types.
- `web/src/lib/api/auth.ts` — `useAuthSession` hook.
- `web/src/lib/api/mutations.ts` — typed `submitMutation` helper.

### Signatures and shapes

```ts
// web/src/lib/api/types.ts
export type RouteId = string & { readonly __brand: 'RouteId' };
export type SnapshotId = string & { readonly __brand: 'SnapshotId' };
export type JsonPointer = string & { readonly __brand: 'JsonPointer' };

export type Upstream = {
  dial: string;            // host:port
  weight?: number;         // 1..=100
};

export type Route = {
  id:             RouteId;
  hostnames:      string[];
  upstreams:      Upstream[];
  path_matchers:  string[];
  headers:        Record<string, string>;
  policy_version: number;
};

export type MutationEnvelope<TBody> = {
  expected_version: number;
  body:             TBody;
};

export type CreateRouteMutation = {
  kind: 'CreateRoute';
  route: Omit<Route, 'id'>;
};

export type UpdateRouteMutation = {
  kind:  'UpdateRoute';
  id:    RouteId;
  patch: Partial<Omit<Route, 'id'>>;
};

export type DeleteRouteMutation = {
  kind: 'DeleteRoute';
  id:   RouteId;
};

export type Mutation =
  | CreateRouteMutation
  | UpdateRouteMutation
  | DeleteRouteMutation;

export type AuthSession =
  | { state: 'loading' }
  | { state: 'unauthenticated' }
  | { state: 'authenticated';   userId: string; role: 'owner'|'operator'|'reader'; mustChangePw: boolean; configVersion: number };
```

```ts
// web/src/lib/api/auth.ts
export function useAuthSession(): AuthSession;

export async function login(username: string, password: string): Promise<{ kind: 'ok'; session: AuthSession & { state: 'authenticated' } }
                                                                       | { kind: 'must-change-password' }
                                                                       | { kind: 'rate-limited'; retryAfterSeconds: number }
                                                                       | { kind: 'invalid' }>;

export async function logout(): Promise<void>;

export async function changePassword(oldPassword: string, newPassword: string): Promise<{ kind: 'ok' } | { kind: 'invalid'; reason: string }>;
```

```ts
// web/src/lib/api/mutations.ts
export async function submitMutation<TBody>(
  envelope: MutationEnvelope<TBody>,
): Promise<
  | { kind: 'applied'; snapshotId: SnapshotId; configVersion: number }
  | { kind: 'conflict'; currentVersion: number; expectedVersion: number }
  | { kind: 'invalid'; detail: string }
  | { kind: 'apply-failed'; detail: string }
>;
```

### Algorithm

1. `useAuthSession`: a context-backed hook. On mount it issues `GET /api/v1/health` to confirm the daemon is up, then `GET /api/v1/snapshots?limit=1` (any authenticated GET) to probe for a session. A 401 transitions to `unauthenticated`; a 200 returns `authenticated`.
2. `login` POSTs to `/api/v1/auth/login`; the discriminated union maps to the daemon's response codes (200 ok, 409 must-change, 429 rate-limited, 401 invalid).
3. `submitMutation` posts the envelope and discriminates on response codes per slice 9.7.

### Tests

- `web/src/lib/api/auth.test.ts` — three login outcomes with mocked fetch.
- `web/src/lib/api/mutations.test.ts` — 200, 409, 422, 502 responses produce the matching discriminated variant.
- `web/src/lib/api/types.test-d.ts` — `expectTypeOf<Mutation>().toMatchTypeOf<{ kind: string }>()` and per-variant payload checks.

### Acceptance command

`pnpm typecheck && pnpm test --run web/src/lib/api/auth.test.ts web/src/lib/api/mutations.test.ts`

### Exit conditions

- `MutationEnvelope<TBody>` MUST require `expected_version: number`.
- Every login outcome from slice 9.5 MUST be a distinct variant.

### Audit kinds emitted

None directly.

### Tracing events emitted

None directly.

### Cross-references

- PRD T1.6, T1.8, T1.14.

---

## Slice 11.4 [standard] — Login page, bootstrap step-up, change-password page

### Goal

Implement `LoginPage`, `BootstrapPage` (a thin wrapper that surfaces the bootstrap step-up message), and `ChangePasswordPage`. Bootstrap step-up redirects to `/change-password` before any other UI is reachable.

### Entry conditions

- Slice 11.3 done.

### Files to create or modify

- `web/src/features/auth/LoginPage.tsx`.
- `web/src/features/auth/BootstrapPage.tsx`.
- `web/src/features/auth/ChangePasswordPage.tsx`.
- `web/src/features/auth/LoginPage.test.tsx`, `BootstrapPage.test.tsx`, `ChangePasswordPage.test.tsx`.

### Signatures and shapes

```tsx
export function LoginPage(): JSX.Element;
export function BootstrapPage(): JSX.Element;
export function ChangePasswordPage(): JSX.Element;
```

### Algorithm

1. `LoginPage` renders a username/password form. On submit it invokes `login` (slice 11.3).
   - On `ok`, navigate to `/`.
   - On `must-change-password`, navigate to `/change-password`.
   - On `rate-limited`, render a non-dismissable banner with the retry-after.
   - On `invalid`, render a field-level error.
2. `BootstrapPage` is a fixed-string component that renders "Welcome to Trilithon. Please log in with the bootstrap credentials written to `bootstrap-credentials.txt` in your data directory." with a link to `/login`.
3. `ChangePasswordPage` renders a form with `oldPassword`, `newPassword`, `confirmNewPassword`. Client-side rules:
   - `newPassword.length >= 12`.
   - `newPassword !== oldPassword`.
   - `newPassword === confirmNewPassword`.
   On submit, invokes `changePassword`. On `ok`, navigate to `/`. On `invalid`, render the daemon's reason.

### Tests

- `LoginPage.test.tsx::shows_field_errors_on_invalid` — submit empty; assert two field errors.
- `LoginPage.test.tsx::redirects_on_ok` — mock `login` to return `ok`; assert navigation to `/`.
- `LoginPage.test.tsx::redirects_to_change_password_on_bootstrap` — mock returning `must-change-password`; assert navigation to `/change-password`.
- `LoginPage.test.tsx::shows_rate_limit_banner` — mock returning `rate-limited`; assert banner with retry-after.
- `ChangePasswordPage.test.tsx::enforces_minimum_length` — type 8-char password; assert disabled submit and error.
- `ChangePasswordPage.test.tsx::redirects_after_success` — mock `ok`; assert navigation to `/`.

### Acceptance command

`pnpm test --run web/src/features/auth/`

### Exit conditions

- A user MUST be able to log in with bootstrap credentials and be required to change the password before any other UI is reachable.
- `ChangePasswordPage` MUST enforce client-side validation that mirrors slice 9.5's server-side rules.

### Audit kinds emitted

`auth.login-succeeded`, `auth.login-failed` (from the daemon).

### Tracing events emitted

None directly.

### Cross-references

- PRD T1.13, T1.14.
- Hazard H13.
- Architecture §11.

---

## Slice 11.5 [standard] — Dashboard with capability summary, drift banner, apply-in-flight banner

### Goal

`DashboardPage` renders the capability probe summary, a drift-state banner with three states (zero, one, many drifts — V1 surfaces only the latest event but the banner copy reads "drift detected" or "no drift"), and an apply-in-flight banner when a mutation is currently propagating.

### Entry conditions

- Slice 11.3 done.

### Files to create or modify

- `web/src/features/dashboard/DashboardPage.tsx`.
- `web/src/features/dashboard/DashboardPage.test.tsx`.
- `web/src/features/dashboard/CapabilityCard.tsx`.
- `web/src/features/dashboard/DriftBanner.tsx`.
- `web/src/features/dashboard/ApplyInFlightBanner.tsx`.

### Signatures and shapes

```tsx
export function DashboardPage(): JSX.Element;

export type CapabilityCardProps = {
  caddyVersion: string;
  modules:      string[];
  hasRateLimit: boolean;
  hasWaf:       boolean;
};
export function CapabilityCard(props: CapabilityCardProps): JSX.Element;

export type DriftBannerProps =
  | { state: 'clean' }
  | { state: 'drifted'; eventId: string; detectedAt: number };
export function DriftBanner(props: DriftBannerProps): JSX.Element;

export type ApplyInFlightBannerProps = { visible: boolean };
export function ApplyInFlightBanner(props: ApplyInFlightBannerProps): JSX.Element;
```

### Algorithm

1. `DashboardPage` issues three GETs in parallel: `/api/v1/capabilities`, `/api/v1/drift/current`, and a synthetic `apply-in-flight` flag. The flag derives from the Phase 9 health endpoint extended with an `apply_in_flight: boolean` field — if Phase 9 has not added this field, the dashboard treats it as `false` and the banner only renders during local mutation submission.
2. The capability card renders the Caddy version and a list of available optional modules with green check or grey dash.
3. The drift banner renders amber when `state === 'drifted'` with a "Resolve" link to the resolution flow (Phase 12 surface; the link is a placeholder in V1).
4. The apply-in-flight banner renders a slim blue bar across the top.

### Tests

- `DashboardPage.test.tsx::renders_clean_state` — mock 204 for drift; assert no drift banner.
- `DashboardPage.test.tsx::renders_drift_banner` — mock a drift response; assert the banner copy.
- `DashboardPage.test.tsx::renders_apply_in_flight` — set `applyInFlight=true`; assert the banner.
- `CapabilityCard.test.tsx::shows_unavailable_for_missing_modules` — `hasRateLimit=false`; assert the rate-limit row reads "unavailable on this Caddy build".

### Acceptance command

`pnpm test --run web/src/features/dashboard/`

### Exit conditions

- All three banner states render.
- The capability card MUST clearly mark unavailable optional modules.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.4, T1.11, T1.13.
- Architecture §4.11.

---

## Slice 11.6 [standard] — Routes index with pagination

### Goal

`RoutesIndex` lists routes with hostname, primary upstream, status, TLS-badge placeholder, and upstream-health-badge placeholder. Pagination renders 25 routes per page.

### Entry conditions

- Slice 11.3 done.

### Files to create or modify

- `web/src/features/routes/RoutesIndex.tsx`.
- `web/src/features/routes/RoutesIndex.test.tsx`.

### Signatures and shapes

```tsx
export function RoutesIndex(): JSX.Element;

export type RouteRowStatus = 'enabled' | 'disabled' | 'errored';

export type RouteRowProps = {
  route:         Route;
  status:        RouteRowStatus;
  tlsBadge:      'unknown';     // V1 placeholder; Phase 14 fills
  upstreamBadge: 'unknown';     // V1 placeholder; Phase 14 fills
};
export function RouteRow(props: RouteRowProps): JSX.Element;
```

### Algorithm

1. The page fetches routes via the API. The Phase 9 surface does not currently expose a routes-list endpoint; routes are derived from the latest snapshot's `desired_state_json`. The frontend reads the latest snapshot and projects routes from `desired_state.routes`.
2. Pagination is client-side over the projected list at 25 rows per page. (Server-side route pagination is a Phase 14 follow-up.)
3. Status: `enabled` if the route has at least one upstream and is not marked `disabled` in `desired_state`; `disabled` if explicitly disabled; `errored` if the latest audit row for the route id has `outcome = "error"`.

### Tests

- `RoutesIndex.test.tsx::renders_25_per_page` — fixture of 50 routes; assert two pages with 25 each.
- `RoutesIndex.test.tsx::renders_status_badges` — three routes (enabled, disabled, errored); assert badges.
- `RoutesIndex.test.tsx::renders_placeholders_for_tls_and_upstream_health` — assert the placeholders read "unknown" for V1.

### Acceptance command

`pnpm test --run web/src/features/routes/RoutesIndex.test.tsx`

### Exit conditions

- 25 routes per page MUST be the default.
- TLS and upstream-health badges MUST render as placeholders in V1.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.8, T1.9, T1.10.
- Architecture §4.6.

---

## Slice 11.7 [standard] — Route detail, history strip, delete with confirmation

### Goal

`RouteDetail` shows the current desired state for a route plus a strip of recent snapshots affecting that route. `DeleteRouteButton` requires the user to type the hostname before the destructive action proceeds, then submits a `DeleteRoute` mutation.

### Entry conditions

- Slice 11.6 done.

### Files to create or modify

- `web/src/features/routes/RouteDetail.tsx`, `RouteDetail.test.tsx`.
- `web/src/features/routes/DeleteRouteButton.tsx`, `DeleteRouteButton.test.tsx`.
- `web/src/features/routes/HistoryStrip.tsx`.

### Signatures and shapes

```tsx
export function RouteDetail(props: { routeId: RouteId }): JSX.Element;

export type DeleteRouteButtonProps = {
  routeId:         RouteId;
  expectedVersion: number;
};
export function DeleteRouteButton(props: DeleteRouteButtonProps): JSX.Element;

export type HistoryStripProps = {
  routeId:   RouteId;
  snapshots: SnapshotSummary[];
};
export function HistoryStrip(props: HistoryStripProps): JSX.Element;

export type SnapshotSummary = {
  id:             SnapshotId;
  configVersion:  number;
  createdAt:      number;
  intent:         string;
  actorKind:      string;
  actorId:        string;
};
```

### Algorithm

1. `RouteDetail` fetches `GET /api/v1/snapshots/<latest>` and extracts the route from `desired_state`. If absent, render a 404 message.
2. The history strip pulls the last 10 snapshots whose `desired_state` contains the route id, derived by walking `GET /api/v1/snapshots?limit=50` and filtering client-side.
3. `DeleteRouteButton`:
   1. First click reveals a confirmation form: "Type the hostname to confirm: `<hostname>`". The button remains disabled until the typed hostname matches.
   2. On confirm, submit `MutationEnvelope<DeleteRouteMutation>` with `expected_version`.
   3. On `applied`, navigate to `/routes`. On `conflict`, render an inline message with a refresh CTA.

### Tests

- `RouteDetail.test.tsx::renders_history_strip` — fixture with three matching snapshots; assert the strip lists three entries newest-first.
- `RouteDetail.test.tsx::renders_404_for_missing_route` — empty `desired_state`; assert the 404 copy.
- `DeleteRouteButton.test.tsx::confirmation_gate` — initially disabled; type a wrong hostname; still disabled; type the correct hostname; enabled.
- `DeleteRouteButton.test.tsx::submits_delete_mutation` — mock `submitMutation`; assert envelope payload and `expected_version`.
- `DeleteRouteButton.test.tsx::renders_conflict_message_on_409` — mock conflict response; assert the inline message.

### Acceptance command

`pnpm test --run web/src/features/routes/RouteDetail.test.tsx web/src/features/routes/DeleteRouteButton.test.tsx`

### Exit conditions

- Deletion MUST require typed-confirmation of the hostname.
- A successful delete MUST submit a `DeleteRoute` mutation with `expected_version`.

### Audit kinds emitted

`mutation.applied` (daemon-side on success), `mutation.conflicted` (on stale version).

### Tracing events emitted

None directly.

### Cross-references

- PRD T1.8.
- Architecture §6.6, §7.1.

---

## Slice 11.8 [standard] — Route create and update forms with field-level validation

### Goal

`CreateRouteForm` and `UpdateRouteForm` accept hostname, upstream targets, optional path matchers, and optional headers. Client-side validation gates submission. Update produces a diff preview before submission. The Apply button is disabled until the form is valid.

### Entry conditions

- Slice 11.7 done.
- Slice 11.9 ships the diff preview (this slice imports it; ordering is logical: slice 11.8 implements the forms, slice 11.9 the diff component, but they are co-developed; for ship sequencing, ship 11.8 first with a placeholder text-diff and replace once 11.9 lands).

### Files to create or modify

- `web/src/features/routes/CreateRouteForm.tsx`, `CreateRouteForm.test.tsx`.
- `web/src/features/routes/UpdateRouteForm.tsx`, `UpdateRouteForm.test.tsx`.
- `web/src/lib/validation/hostname.ts`, `port.ts`, `regex.ts`.
- `web/src/lib/validation/index.test.ts`.
- `web/eslint.config.js` — add a rule banning `styled-components` and `@emotion/*` imports.

### Signatures and shapes

```ts
// web/src/lib/validation/hostname.ts
export const HOSTNAME_REGEX = /^(?=.{1,253}$)([a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)(\.([a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?))*$/;
export function validateHostname(value: string): { ok: true } | { ok: false; reason: string };

// web/src/lib/validation/port.ts
export function validatePort(value: number): { ok: true } | { ok: false; reason: string };

// web/src/lib/validation/regex.ts
export function validateRegex(value: string): { ok: true; compiled: RegExp } | { ok: false; reason: string };
```

```tsx
export type CreateRouteFormProps = {
  onSubmit: (m: CreateRouteMutation) => Promise<void>;
};
export function CreateRouteForm(props: CreateRouteFormProps): JSX.Element;

export type UpdateRouteFormProps = {
  initial:  Route;
  onSubmit: (m: UpdateRouteMutation) => Promise<void>;
};
export function UpdateRouteForm(props: UpdateRouteFormProps): JSX.Element;
```

### Algorithm

1. The forms maintain a `formState` keyed by field, with per-field `error: string | null`.
2. On every change, the relevant validator runs. The Apply button is disabled while any error is non-null OR any required field is blank.
3. `validateHostname` returns `{ ok: false, reason: "RFC 952 / RFC 1123 violation" }` on miss.
4. `validatePort` checks `Number.isInteger(p) && p >= 1 && p <= 65535`.
5. `validateRegex` runs `new RegExp(value)` in a `try/catch`; on `SyntaxError`, returns `{ ok: false, reason: e.message }`.
6. `UpdateRouteForm` derives `before = props.initial`, `after = computed_route_from_form`. It renders `<DiffPreview before={before} after={after} />` (slice 11.9). Submission goes through `submitMutation`. On `conflict` the form surfaces a banner: "Configuration changed since you started editing. Reload to see current state."
7. The form mirrors the server's `POST /api/v1/desired-state/validate` endpoint by issuing the call on debounce after the user stops typing for 250 ms; the server's response wins for any disagreement. (If Phase 9 has not yet shipped this endpoint, the slice deferes the server-side cross-check to Phase 15; flagged below.)

### Tests

- `web/src/lib/validation/index.test.ts::hostname_valid` — corpus of valid and invalid hostnames.
- `web/src/lib/validation/index.test.ts::port_bounds` — 0, 1, 65535, 65536, NaN.
- `web/src/lib/validation/index.test.ts::regex_invalid_surfaces_message` — `new RegExp("(")` produces the SyntaxError message.
- `CreateRouteForm.test.tsx::apply_disabled_until_valid` — empty form; assert disabled. Fill valid values; assert enabled.
- `CreateRouteForm.test.tsx::submits_create_mutation` — mock `onSubmit`; assert the call carries the correct shape.
- `UpdateRouteForm.test.tsx::renders_diff_preview` — change one field; assert the diff component renders one `modified` entry.
- `UpdateRouteForm.test.tsx::apply_disabled_while_validation_failing` — type an invalid port; assert disabled.

### Acceptance command

`pnpm test --run web/src/features/routes/CreateRouteForm.test.tsx web/src/features/routes/UpdateRouteForm.test.tsx web/src/lib/validation/`

### Exit conditions

- Apply MUST be disabled until every required field is valid.
- Hostname, port, and regex validators MUST surface inline error messages.
- A `styled-components` or `@emotion/*` import MUST fail the lint.

### Audit kinds emitted

`mutation.applied`, `mutation.rejected`, `mutation.conflicted` (daemon side).

### Tracing events emitted

None directly.

### Cross-references

- PRD T1.6, T1.8.
- Architecture §4.6, §11.

---

## Slice 11.9 [standard] — `DiffPreview` component with secret redaction enforcement

### Goal

`DiffPreview` colour-codes additions, removals, and modifications. Secret-marked fields render as `***` plus the ciphertext-derived hash prefix; plaintext is never rendered. The component is the visual surface that closes the loop with Phase 6 (`SecretsRedactor`) and Phase 10 (the vault's secret references).

### Entry conditions

- Slice 11.8 done (or co-developed).

### Files to create or modify

- `web/src/components/diff/DiffPreview.tsx`.
- `web/src/components/diff/DiffPreview.test.tsx`.
- `web/src/components/diff/redact.ts` — client-side redactor that mirrors slice 6.3.
- `web/src/components/diff/__fixtures__/with-secret.json`.

### Signatures and shapes

```tsx
export type DiffPreviewProps = {
  before:        DesiredState;
  after:         DesiredState;
  redactSecrets?: boolean;          // default true
};
export function DiffPreview(props: DiffPreviewProps): JSX.Element;

export type DiffEntry =
  | { kind: 'added';    path: JsonPointer; after:  unknown }
  | { kind: 'removed';  path: JsonPointer; before: unknown }
  | { kind: 'modified'; path: JsonPointer; before: unknown; after: unknown };

export type DesiredState = {
  routes:              Route[];
  unknown_extensions?: Record<JsonPointer, unknown>;
};

// web/src/components/diff/redact.ts
export const SECRET_FIELD_PATTERNS: ReadonlyArray<RegExp>;
export function isSecretPath(path: JsonPointer): boolean;
export function redactValue(value: unknown, hashPrefix: string): string;   // returns "***<prefix>"
```

### Algorithm

1. The component flattens `before` and `after` to `Map<JsonPointer, unknown>`. For each pointer in the symmetric difference, append a `DiffEntry` to a sorted list.
2. Render in a list. Class names per entry:
   - `added` → `bg-emerald-50 text-emerald-900`.
   - `removed` → `bg-rose-50 text-rose-900 line-through`.
   - `modified` → `bg-amber-50 text-amber-900`.
3. For each entry, walk the pre- and post-values. If the path matches `isSecretPath`, replace the value with `redactValue(value, hashPrefix)`. The `hashPrefix` is provided by the server: if the value is `{ "$secret_ref": "<id>" }` (slice 10.7), compute `prefix = sha256(id).slice(0, 12)` and render `***<prefix>`. If `redactSecrets === false` (admin override), render `<unable to display>` rather than the plaintext — the UI MUST never render plaintext secrets.
4. The component MUST NOT call `decrypt` directly; reveal flows through the dedicated reveal UI (a Phase 12 follow-up).

### Tests

- `DiffPreview.test.tsx::renders_added` — fixture with one new key; assert one row with `bg-emerald-50`.
- `DiffPreview.test.tsx::renders_removed` — fixture with one missing key; assert one row with `bg-rose-50`.
- `DiffPreview.test.tsx::renders_modified` — fixture with one changed value; assert one `bg-amber-50` row.
- `DiffPreview.test.tsx::redacts_basic_auth_password` — fixture (`__fixtures__/with-secret.json`) where one route differs in a `$secret_ref`; assert the rendered output includes `***` followed by exactly 12 hex characters and DOES NOT include the literal string `"hunter2"` (the test sets the plaintext through a side channel that is verified absent).
- `DiffPreview.test.tsx::never_renders_plaintext_with_redact_false` — even with `redactSecrets={false}`, plaintext MUST NOT render.

### Acceptance command

`pnpm test --run web/src/components/diff/DiffPreview.test.tsx`

### Exit conditions

- All three diff classes MUST render with distinguishable Tailwind classes.
- Secret-marked fields MUST render `***<12-hex>` and never plaintext.
- Even an explicit override MUST refuse to render plaintext.

### Audit kinds emitted

None directly.

### Tracing events emitted

None directly.

### Cross-references

- PRD T1.8.
- Hazard H10.
- ADR-0014.
- Architecture §6.6.

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration (Rust gate plus `pnpm typecheck && pnpm lint && pnpm format:check && pnpm test --run`).
- [ ] A user who has never touched a configuration file MUST be able to install Trilithon, open `http://127.0.0.1:7878`, log in with bootstrap credentials, change their password, and create their first route (slices 11.4, 11.8).
- [ ] A newly created route serves traffic within five seconds of approval, given a healthy Caddy (covered by Phase 7's apply latency budget plus this phase's submission flow).
- [ ] A deleted route stops serving traffic within five seconds (slice 11.7 plus Phase 7).
- [ ] An update is atomic; there is no observable window where the route is half-updated (Phase 7 invariant; Phase 11 surfaces the apply confirmation).
- [ ] Apply is disabled while validation is failing (slice 11.8).
- [ ] No plaintext secret renders in any read view (slice 11.9).

## Open questions

- RESOLVED — `apply_in_flight` is now part of Phase 9 slice 9.1's `/api/v1/health` response. Slice 11.5 reads `apply_in_flight` directly from the health endpoint.
- RESOLVED — `GET /api/v1/routes` is now part of Phase 9 slice 9.8 with cursor pagination, `limit` (default 100, max 500), and `hostname_filter`. Slice 11.6 calls this endpoint instead of deriving from the latest snapshot.
- RESOLVED — `POST /api/v1/desired-state/validate` is owned by Phase 15 slice 15.2. For V1 the dependency is explicit: Phase 11's create/update forms run client-side validators only. The server-authoritative validate endpoint becomes available when Phase 15 ships and the form upgrades non-disruptively (the form already issues a debounced fetch; adding the endpoint at Phase 15 simply makes the fetch return real validation rather than a 404).
- The hash-prefix length 12 in slice 11.9 mirrors slice 6.3's `HASH_PREFIX_LEN`. Whether 12 hex chars is sufficiently collision-resistant for visual disambiguation is unresolved; the project owner SHOULD ratify before V1 ships.

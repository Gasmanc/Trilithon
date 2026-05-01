# Phase 15 — Dual-pane configuration editor

Source of truth: [`../phases/phased-plan.md#phase-15--dual-pane-configuration-editor`](../phases/phased-plan.md#phase-15--dual-pane-configuration-editor).

> **Path-form note.** Backend paths are workspace-relative; rooted at `core/`. Phase 15 introduces `crates/core/src/caddyfile/renderer.rs` (the Caddy JSON → Caddyfile printer) alongside the Phase 13 `crates/core/src/caddyfile/parser.rs`. Web paths: `web/src/features/editor/DualPaneEditor.tsx`, `web/src/features/editor/CaddyfilePane.tsx`, `web/src/features/editor/JsonPane.tsx`, `web/src/features/editor/state.ts` (the editor state machine), `web/src/features/editor/DualPaneEditor.test.tsx`. See [`README.md`](README.md) "Path conventions".

## Pre-flight checklist

- [ ] Phase 13 complete (Caddyfile parser exists; legible form rendering reuses it).
- [ ] Phase 14 complete (TLS and upstream health surfaced).

## Tasks

### Backend / core crate

- [ ] **Implement the read-only Caddyfile renderer.**
  - Acceptance: The renderer MUST convert a `DesiredState` into Caddyfile-style legible text; round-trip with the Phase 13 parser MUST be covered by the same fixture corpus.
  - Done when: `cargo test -p trilithon-core caddyfile::renderer::round_trip` passes.
  - Feature: T1.12.
- [ ] **Document that the legible form is Trilithon-managed.**
  - Acceptance: The renderer's documentation MUST state that comments and ordering are not preserved and that the legible form is a Trilithon-managed rendering rather than a verbatim Caddyfile.
  - Done when: the doc comment is present and surfaced in the in-UI hint.
  - Feature: T1.12.

### HTTP endpoints

- [ ] **Implement `POST /api/v1/desired-state/validate`.**
  - Acceptance: The endpoint contract is exactly:

    ```
    POST /api/v1/desired-state/validate
    Content-Type: application/json
    Body: { format: "caddyfile" | "caddy-json", source: string }
    Response 200: { ok: true,  desired_state: DesiredState }
    Response 422: { ok: false, errors: ValidationError[] }
    ```

    The TypeScript `ValidationError` shape MUST be:

    ```ts
    export type ValidationError = {
      pane:    'caddyfile' | 'caddy-json';
      line?:   number;
      column?: number;
      path?:   JsonPointer;
      rule:    string;          // matches core ValidationRule enum
      message: string;
      hint?:   string;
    };
    ```
  - Done when: integration tests cover both input forms and structured errors.
  - Feature: T1.12.

### Frontend

- [ ] **Implement the side-by-side editor layout.**
  - Path: `web/src/features/editor/DualPaneEditor.tsx`.
  - Acceptance: The editor MUST host a Caddyfile-style pane on the left and a raw Caddy JSON pane on the right. The component signatures MUST be:

    ```tsx
    export function DualPaneEditor(props: {
      initialState: DesiredState;
      onCommit:     (next: DesiredState) => Promise<CommitOutcome>;
      readOnly?:    boolean;
    }): JSX.Element;

    export type CommitOutcome =
      | { ok: true;  applied_at: number; new_version: number }
      | { ok: false; conflict: ConflictResponse }   // 409 from API
      | { ok: false; validation_errors: ValidationError[] };
    ```
  - Done when: a Vitest test asserts the two-pane layout renders.
  - Feature: T1.12.
- [ ] **Implement the JSON pane with syntax highlighting.**
  - Acceptance: The JSON pane MUST be a controlled textarea with minimal client-side syntax highlighting; introducing a Monaco-class editor is OUT OF SCOPE FOR V1.
  - Done when: a Vitest test asserts the controlled-state behaviour.
  - Feature: T1.12.
- [ ] **Implement live cross-validation with debounce (state machine).**
  - Path: `web/src/features/editor/state.ts`.
  - Acceptance: The editor MUST be implemented as the following state machine, in numbered pseudocode:

    ```
    States: Idle, Typing, Debouncing, Validating, ValidOk, ValidErr.
    Transitions:
      Idle → Typing on first keystroke.
      Typing → Debouncing on each keystroke; reset 200 ms timer.
      Debouncing → Validating when timer elapses; cancel in-flight Validating call if any.
      Validating → ValidOk on 200 response: write parsed DesiredState, re-render the inactive pane.
      Validating → ValidErr on 422 response: render structured error list pointing at line/column or JsonPointer.
      Any → Typing on new keystroke (cancels in-flight Validating fetch via AbortController).
    Apply button is disabled in Idle, Typing, Debouncing, Validating, ValidErr; enabled only in ValidOk and only when the parsed state differs from initialState.
    ```
  - Done when: a Vitest test asserts each transition; the AbortController cancellation path is covered by an explicit test.
  - Feature: T1.12.
- [ ] **Gate Apply on validity and on state diff.**
  - Acceptance: The Apply button MUST be disabled in `Idle`, `Typing`, `Debouncing`, `Validating`, `ValidErr` and MUST be enabled only in `ValidOk` and only when the parsed state differs from `initialState`.
  - Done when: a Vitest test asserts the disabled and enabled states.
  - Feature: T1.12.
- [ ] **Reuse the Phase 11 diff preview before commit.**
  - Acceptance: A diff preview MUST render before commit, reusing the Phase 11 component.
  - Done when: a Vitest test renders the preview against a fixture.
  - Feature: T1.12.

### Tests

- [ ] **Vitest test corpus for the dual-pane editor.**
  - Path: `web/src/features/editor/DualPaneEditor.test.tsx`.
  - Acceptance: Vitest tests MUST be named exactly: `caddyfile_pane_validates_on_keystroke`, `json_pane_updates_after_caddyfile_validates`, `apply_button_disabled_in_validating`, `validation_error_renders_with_line_and_column`, `apply_button_enabled_only_when_state_changed`.
  - Done when: every named test passes.
  - Feature: T1.12.

## Cross-references

- ADR-0001 (Caddy as the only supported reverse proxy).
- ADR-0002 (Caddy JSON admin API as source of truth).
- PRD T1.12 (dual-pane configuration editor).
- Architecture: "Dual-pane editor," "Validation pipeline."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] An invalid edit on either side shows a structured error pointing to the offending line and key.
- [ ] Apply is disabled while validation is failing.
- [ ] A valid edit produces a preview diff against current desired state before the user commits.

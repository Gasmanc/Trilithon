# Phase 15 — Dual-pane configuration editor — Implementation Slices

> Phase reference: [../phases/phase-15-dual-pane-editor.md](../phases/phase-15-dual-pane-editor.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [phase-15-dual-pane-editor.md](../phases/phase-15-dual-pane-editor.md).
- Architecture sections: §4.1 (`core` purity), §4.7 (frontend dual-pane editor module group), §6.6 (audit kinds), §7.1 (mutation lifecycle), §12.1 (tracing vocabulary), §13 (performance budget).
- Trait signatures: §6 `core::reconciler::Applier::validate`.
- ADRs: ADR-0001, ADR-0002 (Caddyfile is a one-way import; the legible form on the left pane is a Trilithon-managed rendering, not a verbatim Caddyfile).
- PRD T1.12 (dual-pane configuration editor).

## Glossary specific to this phase

| Term | Definition |
|------|------------|
| Caddyfile pane | The left pane. Hosts a Trilithon-managed Caddyfile-style legible rendering of the current `DesiredState`. Comments and source ordering from any imported Caddyfile are not preserved. |
| JSON pane | The right pane. Hosts the raw Caddy JSON serialisation of the current `DesiredState` as a controlled textarea with minimal client-side syntax highlighting. |
| Active pane | Whichever pane the user most recently typed into. Validation is driven by the active pane's debounce timer; the inactive pane re-renders from the parsed `DesiredState` when validation succeeds. |
| Validation debounce window | A 200 millisecond window between the user's last keystroke and the dispatched validation call. Subsequent keystrokes reset the timer. |
| Editor state machine | The pure-function reducer in `web/src/features/editor/state.ts`. States: `Idle`, `Typing`, `Debouncing`, `Validating`, `ValidOk`, `ValidErr`. Events: `Keystroke`, `DebounceElapsed`, `ValidationOk`, `ValidationErr`, `AbortFinished`. |
| Cross-pane re-render | When validation succeeds, the **inactive** pane is repopulated from the freshly parsed `DesiredState`. The active pane is left untouched. |

## Trait surfaces consumed by this phase

- §6 `core::reconciler::Applier::validate` — invoked by the Phase 15 validation endpoint to run desired-state validation without applying.

No new traits or new trait methods are introduced by Phase 15. The phase reuses Phase 13's renderer, parser, and translator surfaces, and adds two HTTP endpoints in `cli`.

## Cross-cutting invariants

- **The Caddyfile pane is a Trilithon-managed rendering, not a verbatim Caddyfile.** Comments and source ordering are not preserved (ADR-0002). The renderer's doc comment and an inline UI hint state this. Round-trip with the Phase 13 parser produces a structurally equivalent `DesiredState`, not a byte-equivalent file.
- **The JSON pane is a controlled textarea.** Introducing a Monaco-class editor is OUT OF SCOPE FOR V1 (per the phase reference).
- **Apply gating is unconditional.** The Apply button is disabled in every state other than `ValidOk` AND when the parsed state equals `initialState`. There is no override.
- **AbortController cancellation is mandatory.** Any keystroke that arrives while a validation fetch is in flight MUST abort that fetch. A test (slice 15.6) asserts the call.
- **Validation is read-only.** The validation endpoint never writes to the audit log, never enqueues a mutation, and never advances `config_version`.

## Slice plan summary

| # | Slice title | Primary files | Effort (h) | Depends on |
|---|---|---|---|---|
| 15.1 | Caddyfile renderer in `core` | `core/crates/core/src/caddyfile/renderer.rs` | 8 | Phase 13, Phase 14 |
| 15.2 | `POST /api/v1/desired-state/validate` endpoint | `core/crates/cli/src/http/desired_state_validate.rs`, `core/crates/core/src/validation.rs` | 6 | 15.1 |
| 15.3 | Editor state machine in TypeScript | `web/src/features/editor/state.ts`, `web/src/features/editor/state.test.ts` | 5 | 15.2 |
| 15.4 | `DualPaneEditor` shell layout | `web/src/features/editor/DualPaneEditor.tsx`, `web/src/features/editor/CaddyfilePane.tsx`, `web/src/features/editor/JsonPane.tsx` | 6 | 15.3 |
| 15.5 | Apply gating, diff preview, commit handler | `web/src/features/editor/DualPaneEditor.tsx`, integration with Phase 11 diff preview | 5 | 15.4 |
| 15.6 | Dual-pane Vitest test corpus | `web/src/features/editor/DualPaneEditor.test.tsx` | 6 | 15.5 |

After every slice: `cargo build --workspace` succeeds; `pnpm typecheck` succeeds; the slice's named tests pass.

---

## Slice 15.1 [standard] — Caddyfile renderer in `core`

### Goal

Implement the read-only Caddyfile renderer: a pure function converting a `DesiredState` into Caddyfile-style legible text. The output is for human reading on the left pane of the editor; it is **not** a round-trippable Caddyfile in the lossless sense. Comments and source ordering are not preserved (they are not in `DesiredState`). The Phase 13 parser MUST consume the renderer's output successfully for every fixture in the Phase 13 corpus, satisfying the round-trip property at the structural level. The renderer's documentation MUST state these constraints.

### Entry conditions

- Phase 13 complete (parser, AST, and fixture corpus).
- Phase 14 complete (TLS adapter integration is unrelated but bundled per the phase reference's pre-flight checklist).

### Files to create or modify

- `core/crates/core/src/caddyfile/renderer.rs` — `pub fn render(state: &DesiredState) -> String`.
- `core/crates/core/src/caddyfile/mod.rs` — register `pub mod renderer;`.

### Signatures and shapes

```rust
// core/crates/core/src/caddyfile/renderer.rs
use crate::desired_state::DesiredState;

/// Render a `DesiredState` as Caddyfile-style legible text.
///
/// The output is a Trilithon-managed rendering of the desired state, not a
/// verbatim Caddyfile reproduction of any source the user may have imported.
/// Comments and source ordering are not preserved; the renderer emits a
/// canonical ordering (sites alphabetically by primary host; directives in a
/// fixed canonical order documented in this function's body). Round-trip with
/// the Phase 13 parser produces a structurally equivalent `DesiredState`.
pub fn render(state: &DesiredState) -> String;

#[derive(Debug, Default)]
struct RenderContext {
    indent: u32,
    out: String,
}
```

### Algorithm

1. Sort `state.routes` by primary host ascending, then by port.
2. For each route, emit a site block:
   1. Address line: `<host>:<port>` (omit port if 80 for `http://` or 443 for `https://`).
   2. Open brace.
   3. Emit named matchers first (`@name { ... }` blocks).
   4. Emit directives in canonical order: `header_up`, `header_down`, `tls`, `encode`, `log`, `route`, `handle`, `handle_path`, `redir`, `respond`, `file_server`, `reverse_proxy`. (This order is documented inline.)
   5. Emit each directive with two-space indentation.
   6. Close brace.
3. Globals are emitted before site blocks.

The renderer never emits comments. The doc comment states this.

### Tests

Unit tests inline in `core/crates/core/src/caddyfile/renderer.rs`:

- `render_round_trip_corpus_01_trivial` — for every fixture in the `01_trivial` directory, render the fixture's golden `DesiredState`, parse the result via Phase 13, render again, and assert the second render equals the first byte-for-byte (idempotency).
- `render_round_trip_corpus_02_reverse_proxy`.
- `render_round_trip_corpus_03_virtual_hosts`.
- `render_round_trip_corpus_04_path_matchers`.
- `render_round_trip_corpus_06_snippets` — note: snippet expansion happens during import; the rendered output never contains `import` directives.
- `render_emits_canonical_directive_order`.
- `render_sorts_sites_alphabetically_by_host`.

The renderer is exercised against the Phase 13 corpus; `cargo test -p trilithon-core caddyfile::renderer::round_trip` is the umbrella test name from the phase reference.

### Acceptance command

`cargo test -p trilithon-core caddyfile::renderer`

### Exit conditions

- All seven tests pass.
- The renderer's doc comment states that comments and ordering are not preserved.
- The legible form is a Trilithon-managed rendering rather than a verbatim Caddyfile.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.12.
- ADR-0002 (Caddyfile is one-way; the renderer is for display only).

---

## Slice 15.2 [cross-cutting] — `POST /api/v1/desired-state/validate` endpoint

### Goal

Ship the validation endpoint consumed by both editor panes. Accepts `format: "caddyfile" | "caddy-json"` and a `source: string`. Returns the parsed `DesiredState` on success and a structured error list on failure. The endpoint never mutates state; it is the read-only validation surface the editor calls on every debounced keystroke.

### Entry conditions

- Slice 15.1 complete.
- The Phase 13 parser is callable from `cli`.
- `Applier::validate` per trait-signatures.md §6 is implemented.

### Files to create or modify

- `core/crates/cli/src/http/desired_state_validate.rs`.
- `core/crates/core/src/validation.rs` — `ValidationError` shape (already exists from Phase 4; extend with `pane`).

### Signatures and shapes

```rust
// core/crates/cli/src/http/desired_state_validate.rs

#[derive(serde::Deserialize)]
pub struct ValidateBody {
    pub format: ValidateFormat,
    pub source: String,
}

#[derive(serde::Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum ValidateFormat { Caddyfile, CaddyJson }

#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum ValidateResponse {
    Ok { ok: bool /* always true */, desired_state: DesiredState },
    Err { ok: bool /* always false */, errors: Vec<ValidationError> },
}

#[derive(serde::Serialize, Clone)]
pub struct ValidationError {
    pub pane: ValidatePane,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub path: Option<String>,           // JSON Pointer
    pub rule: String,                   // matches core::validation::ValidationRule enum name
    pub message: String,
    pub hint: Option<String>,
}

#[derive(serde::Serialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum ValidatePane { Caddyfile, CaddyJson }

pub async fn post_validate(
    State(ctx): State<HttpContext>,
    auth: AuthenticatedActor,
    Json(body): Json<ValidateBody>,
) -> Result<(StatusCode, Json<ValidateResponse>), ApiError>;
```

### Algorithm

1. Authenticate session.
2. Open span `http.request.received`.
3. Branch on `body.format`:
   1. `Caddyfile`: lex, parse, expand, translate via `core::caddyfile`. Then build a `DesiredState` by applying the resulting mutations to an empty state. Map any error with `pane: Caddyfile` and the lexer/parser/translator's line/column.
   2. `CaddyJson`: deserialise via `serde_json` into `DesiredState`. Map deserialisation errors with `pane: CaddyJson` and the offending JSON Pointer (use `serde_path_to_error`).
4. Run `core::validation::validate(&desired_state, &capability_set)`. Map any rule violation to `ValidationError`.
5. On success, return `(200, ValidateResponse::Ok { ok: true, desired_state })`.
6. On any failure, return `(422, ValidateResponse::Err { ok: false, errors })`.

### Tests

Integration tests at `core/crates/cli/tests/desired_state_validate.rs`:

- `validate_caddyfile_returns_200_and_desired_state`.
- `validate_caddyfile_returns_422_with_pane_caddyfile_and_line_column`.
- `validate_caddy_json_returns_200_and_desired_state`.
- `validate_caddy_json_returns_422_with_pane_caddy_json_and_path`.
- `validate_rejects_unauthenticated_request`.
- `validate_rejects_unknown_format_with_400`.

### Acceptance command

`cargo test -p trilithon-cli --test desired_state_validate`

### Exit conditions

- All six tests pass.
- `ValidationError` carries `pane`, optional `line`/`column`, optional `path`, `rule`, `message`, optional `hint`.
- The endpoint is read-only; no audit row is written.

### Audit kinds emitted

None.

### Tracing events emitted

`http.request.received`, `http.request.completed`.

### Cross-references

- PRD T1.12.

---

## Slice 15.3 [trivial] — Editor state machine in TypeScript

### Goal

Implement the editor state machine described in the phase reference, in pure TypeScript without React, so it is independently unit-tested. States: `Idle`, `Typing`, `Debouncing`, `Validating`, `ValidOk`, `ValidErr`. Transitions are driven by typed events: `Keystroke`, `DebounceElapsed`, `ValidationOk`, `ValidationErr`, `AbortFinished`.

### Entry conditions

- Slice 15.2 complete.

### Files to create or modify

- `web/src/features/editor/state.ts`.
- `web/src/features/editor/state.test.ts`.

### Signatures and shapes

```ts
// web/src/features/editor/state.ts
export type EditorState =
  | { kind: 'idle' }
  | { kind: 'typing'; pendingSource: string; pane: Pane }
  | { kind: 'debouncing'; pendingSource: string; pane: Pane; timer: number }
  | { kind: 'validating'; pendingSource: string; pane: Pane; abort: AbortController }
  | { kind: 'valid-ok'; parsed: DesiredState; lastSource: string; pane: Pane }
  | { kind: 'valid-err'; errors: readonly ValidationError[]; lastSource: string; pane: Pane };

export type Pane = 'caddyfile' | 'caddy-json';

export type EditorEvent =
  | { type: 'keystroke'; pane: Pane; source: string }
  | { type: 'debounce-elapsed' }
  | { type: 'validation-ok'; parsed: DesiredState }
  | { type: 'validation-err'; errors: readonly ValidationError[] }
  | { type: 'abort-finished' };

export interface EditorEffect {
  readonly kind: 'start-debounce' | 'cancel-debounce' | 'start-validate' | 'abort-validate';
}

export interface ReducerOutput {
  readonly state: EditorState;
  readonly effects: readonly EditorEffect[];
}

export const INITIAL_STATE: EditorState = { kind: 'idle' };
export const DEBOUNCE_MS = 200;

export function reduce(state: EditorState, event: EditorEvent): ReducerOutput;
```

### Algorithm

The reducer follows the phase reference's transition table:

1. `idle` + `keystroke` → `typing` with `pendingSource`. Effects: `start-debounce`.
2. `typing` + `keystroke` → `debouncing`. Effects: `cancel-debounce`, `start-debounce`.
3. `debouncing` + `keystroke` → remains `debouncing` with new source. Effects: `cancel-debounce`, `start-debounce`.
4. `debouncing` + `debounce-elapsed` → `validating`. Effects: `start-validate`.
5. `validating` + `keystroke` → `typing` with new source. Effects: `abort-validate`, `start-debounce`.
6. `validating` + `validation-ok` → `valid-ok`. Effects: none.
7. `validating` + `validation-err` → `valid-err`. Effects: none.
8. `valid-ok` / `valid-err` + `keystroke` → `typing`. Effects: `start-debounce`.

The reducer is pure; the host (slice 15.4) is responsible for wiring effects to `setTimeout`, `fetch`, and `AbortController`.

### Tests

Vitest tests at `web/src/features/editor/state.test.ts`:

- `idle_to_typing_on_first_keystroke`.
- `typing_to_debouncing_resets_timer`.
- `debouncing_to_validating_on_timer_elapsed`.
- `validating_to_typing_on_keystroke_emits_abort_validate`.
- `validating_to_valid_ok_on_validation_ok`.
- `validating_to_valid_err_on_validation_err`.
- `valid_ok_to_typing_on_new_keystroke`.

### Acceptance command

`pnpm vitest run web/src/features/editor/state.test.ts`

### Exit conditions

- All seven tests pass.
- The reducer is pure; no side effects.
- The `DEBOUNCE_MS` constant is `200` and is exported.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.12.

---

## Slice 15.4 [standard] — `DualPaneEditor` shell layout

### Goal

Wire the state machine into a React component. Two panes side by side: Caddyfile-style on the left, raw Caddy JSON on the right. The JSON pane is a controlled `<textarea>` with minimal client-side syntax highlighting (a CSS-class-per-token highlighter); a Monaco-class editor is OUT OF SCOPE FOR V1. On a valid edit in either pane, the *other* pane re-renders from the parsed `DesiredState`.

### Entry conditions

- Slice 15.3 complete.

### Files to create or modify

- `web/src/features/editor/DualPaneEditor.tsx`.
- `web/src/features/editor/CaddyfilePane.tsx`.
- `web/src/features/editor/JsonPane.tsx`.
- `web/src/features/editor/highlighter.ts` — minimal token highlighter for JSON.

### Signatures and shapes

```tsx
// web/src/features/editor/DualPaneEditor.tsx
export type CommitOutcome =
  | { ok: true; applied_at: number; new_version: number }
  | { ok: false; conflict: ConflictResponse }
  | { ok: false; validation_errors: readonly ValidationError[] };

export interface DualPaneEditorProps {
  readonly initialState: DesiredState;
  readonly onCommit: (next: DesiredState) => Promise<CommitOutcome>;
  readonly readOnly?: boolean;
}

export function DualPaneEditor(props: DualPaneEditorProps): JSX.Element;
```

```tsx
// web/src/features/editor/CaddyfilePane.tsx
export interface CaddyfilePaneProps {
  readonly value: string;
  readonly errors: readonly ValidationError[];
  readonly onChange: (next: string) => void;
  readonly readOnly: boolean;
}
export function CaddyfilePane(props: CaddyfilePaneProps): JSX.Element;
```

```tsx
// web/src/features/editor/JsonPane.tsx
export interface JsonPaneProps {
  readonly value: string;                  // controlled textarea content
  readonly errors: readonly ValidationError[];
  readonly onChange: (next: string) => void;
  readonly readOnly: boolean;
}
export function JsonPane(props: JsonPaneProps): JSX.Element;
```

### Algorithm

1. The `DualPaneEditor` holds two strings: `caddyfileSource` and `jsonSource`. Each pane is controlled.
2. Each pane has an independent state-machine instance from slice 15.3.
3. On a keystroke in one pane:
   1. Dispatch `keystroke` to that pane's reducer.
   2. The host effect handler clears the previous debounce timer and starts a new 200 ms `setTimeout`.
   3. When the timer elapses, dispatch `debounce-elapsed`. The effect handler creates an `AbortController` and calls `fetch('/api/v1/desired-state/validate', { signal })`.
   4. On `200`, dispatch `validation-ok` and re-render the **other** pane from the parsed `DesiredState` (via `core::caddyfile::renderer` for the Caddyfile pane; via `JSON.stringify(state, null, 2)` for the JSON pane). The other pane's source is the freshly rendered string; this MUST NOT re-trigger validation in the other pane (compare against the last validated source).
   5. On `422`, dispatch `validation-err` with the structured errors and render error markers next to the offending line/column or path.
4. The Caddyfile-rendering call from the JSON pane's `valid-ok` effect goes through a small WASM build of the Phase 13 renderer OR (preferred for V1) calls `POST /api/v1/desired-state/render-caddyfile` (a small read endpoint added in this slice for that purpose).

### Algorithm — preventing re-render feedback loops

The cross-pane re-render is the single most defect-prone area of the dual-pane editor. The following rules prevent feedback loops:

1. The `DualPaneEditor` holds, per pane, a `lastValidatedSource: string`. This is the source string that produced the most recent `valid-ok` parse.
2. When the inactive pane is repopulated from a freshly parsed `DesiredState`, the host first writes the rendered string to `lastValidatedSource` for that pane, THEN updates the controlled value. The pane's reducer receives no `keystroke` event because the change came from the host, not from the user.
3. A pane MUST NOT dispatch `keystroke` on a value change driven by the host. The implementation distinguishes user input (an `<onChange>` event from the textarea) from host updates (a programmatic `setValue` call).
4. The active pane is locked. A re-render of the active pane from a parse triggered by the same pane is a no-op: the rendered string equals the validated source.

### Algorithm — error-marker rendering

1. The Caddyfile pane renders a gutter column to the left of every line.
2. Each `ValidationError { pane: 'caddyfile', line, column, ... }` produces a red marker on the corresponding gutter cell with an aria-label containing the message and an aria-describedby reference to a tooltip carrying the optional `hint`.
3. The JSON pane renders structural errors at the matching JSON Pointer location. The implementation walks the textarea text and computes the `(line, column)` corresponding to the JSON Pointer using a small JSON-position scanner; the result drives the same gutter-marker rendering as the Caddyfile pane.
4. If multiple errors share a line, the gutter marker shows the count; the tooltip lists every error on that line.

The render endpoint is a small addition; document it inline:

```rust
// core/crates/cli/src/http/desired_state_validate.rs (extension)
pub async fn post_render_caddyfile(
    State(ctx): State<HttpContext>,
    auth: AuthenticatedActor,
    Json(state): Json<DesiredState>,
) -> Result<(StatusCode, Json<RenderResponse>), ApiError>;

#[derive(serde::Serialize)]
pub struct RenderResponse { pub caddyfile: String }
```

### Tests

Vitest tests in `web/src/features/editor/DualPaneEditor.test.tsx` (the comprehensive corpus lands in slice 15.6; this slice ships a smoke test):

- `dual_pane_editor_renders_two_panes_with_initial_state`.
- `dual_pane_editor_jsonpane_is_controlled`.

### Acceptance command

`pnpm vitest run web/src/features/editor/DualPaneEditor.test.tsx -t "renders_two_panes_with_initial_state|jsonpane_is_controlled"`

### Exit conditions

- The two smoke tests pass.
- The component renders without runtime errors.
- `pnpm typecheck` and `pnpm lint` pass.

### Audit kinds emitted

None.

### Tracing events emitted

None directly.

### Cross-references

- PRD T1.12.

---

## Slice 15.5 [standard] — Apply gating, diff preview, commit handler

### Goal

Wire the Apply button, gate it on `valid-ok` AND a non-empty diff against `initialState`, and reuse the Phase 11 diff preview component before commit. On Apply, call `props.onCommit(parsed)`; surface the typed `CommitOutcome`. A `409 Conflict` from the API surfaces inline with a "Rebase" call-to-action that is implemented by Phase 17 (this slice renders the call-to-action but its handler is a no-op until Phase 17 is shipped).

### Entry conditions

- Slice 15.4 complete.
- The Phase 11 diff preview is exported (`web/src/components/DiffPreview.tsx`).

### Files to create or modify

- `web/src/features/editor/DualPaneEditor.tsx` — extend with the Apply button and commit flow.
- `web/src/features/editor/DiffPreviewModal.tsx` — modal hosting the Phase 11 diff preview.

### Signatures and shapes

```tsx
// web/src/features/editor/DiffPreviewModal.tsx
export interface DiffPreviewModalProps {
  readonly before: DesiredState;
  readonly after: DesiredState;
  readonly onConfirm: () => Promise<void>;
  readonly onCancel: () => void;
}
export function DiffPreviewModal(props: DiffPreviewModalProps): JSX.Element;
```

### Algorithm — Apply gating

1. The Apply button is disabled when:
   - `props.readOnly === true`, OR
   - The active pane's state is not `valid-ok`, OR
   - The parsed `DesiredState` deep-equals `props.initialState` (no changes).
2. On click, open `DiffPreviewModal` with `before = initialState` and `after = parsed`.
3. On confirm, call `props.onCommit(parsed)`.
4. On `CommitOutcome.ok === true`, close the modal and toast success.
5. On `ok === false` with `conflict`, render the conflict envelope and a "Rebase" call-to-action (Phase 17 will wire its handler).
6. On `ok === false` with `validation_errors`, render them in the offending pane (this case occurs when server-side validation rejects something the client-side validation accepted).

### Tests

Vitest tests in `web/src/features/editor/DualPaneEditor.test.tsx`:

- `apply_button_disabled_when_read_only`.
- `apply_button_disabled_when_state_unchanged`.
- `apply_button_disabled_when_validation_failing`.
- `apply_opens_diff_preview_modal`.
- `apply_calls_on_commit_with_parsed_state`.
- `apply_renders_conflict_callout_on_409`.

### Acceptance command

`pnpm vitest run web/src/features/editor/DualPaneEditor.test.tsx -t apply_`

### Exit conditions

- All six tests pass.
- The Apply button is disabled in `Idle`, `Typing`, `Debouncing`, `Validating`, `ValidErr`, and in `ValidOk` when the state is unchanged.
- The diff preview reuses the Phase 11 component.

### Audit kinds emitted

None directly. The `onCommit` callback dispatches a mutation that produces the standard Phase 7 audit rows.

### Tracing events emitted

None directly.

### Cross-references

- PRD T1.12.
- Phase 11 (diff preview component).
- Phase 17 (rebase handler).

---

## Slice 15.6 [trivial] — Dual-pane Vitest test corpus

### Goal

Ship the named test corpus required by the phase reference and additional coverage for the cross-pane re-render and the AbortController cancellation path.

### Entry conditions

- Slices 15.4 and 15.5 complete.

### Files to create or modify

- `web/src/features/editor/DualPaneEditor.test.tsx` — extend with the named corpus.
- `web/src/features/editor/fixtures/` — fixture `DesiredState` JSON and corresponding rendered Caddyfile strings.

### Signatures and shapes

No new public types. Tests use a stub `runValidate` adapter injected through React context per the existing project test scaffolding.

### Algorithm

For each test, mount `<DualPaneEditor>` inside a context provider that supplies a programmable validation adapter. The adapter records every `runValidate` call and returns programmable `Promise` resolutions.

### Tests

Required by the phase reference (names verbatim):

- `caddyfile_pane_validates_on_keystroke` — type into the Caddyfile pane; advance fake timers by 200 ms; assert the validation adapter was called once with `format: 'caddyfile'`.
- `json_pane_updates_after_caddyfile_validates` — type into the Caddyfile pane; resolve validation with a `DesiredState` whose JSON differs from the initial; assert the JSON pane's textarea value is the freshly serialised JSON.
- `apply_button_disabled_in_validating` — type into either pane; advance to `validating` (do not resolve); assert Apply is disabled.
- `validation_error_renders_with_line_and_column` — type invalid Caddyfile; resolve validation with `422` and a `ValidationError { line: 5, column: 12, ... }`; assert the error marker is visible at line 5.
- `apply_button_enabled_only_when_state_changed` — initial state and parsed state are deep-equal; assert Apply disabled. Make a change; resolve `valid-ok`; assert Apply enabled.

Additional tests (cross-pane and abort coverage):

- `keystroke_during_validation_aborts_in_flight_fetch` — type, advance to `validating`, then type again; assert the in-flight fetch's `AbortController.abort` was called exactly once.
- `cross_pane_render_does_not_re_trigger_validation_on_other_pane` — assert that when the JSON pane updates from a Caddyfile validation, the JSON pane's reducer does not enter `typing`.
- `aria_attributes_present_on_error_markers` — accessibility smoke check.

### Acceptance command

`pnpm vitest run web/src/features/editor/DualPaneEditor.test.tsx`

### Exit conditions

- All eight tests pass with the names listed (the five required names match the phase reference verbatim).
- `pnpm lint` and `pnpm typecheck` pass.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.12.

---

## Verification matrix

Every Phase 15 acceptance bar maps to a specific test. The table below lets the implementer cross-check completeness before declaring the phase shipped.

| Acceptance bar | Slice | Test name | Status |
|---|---|---|---|
| Caddyfile pane validates on keystroke | 15.6 | `caddyfile_pane_validates_on_keystroke` | required |
| JSON pane updates after Caddyfile validates | 15.6 | `json_pane_updates_after_caddyfile_validates` | required |
| Apply disabled while validating | 15.6 | `apply_button_disabled_in_validating` | required |
| Validation error renders with line and column | 15.6 | `validation_error_renders_with_line_and_column` | required |
| Apply enabled only when state changed | 15.6 | `apply_button_enabled_only_when_state_changed` | required |
| Keystroke during validation aborts in-flight fetch | 15.6 | `keystroke_during_validation_aborts_in_flight_fetch` | required |
| Cross-pane render does not re-trigger validation | 15.6 | `cross_pane_render_does_not_re_trigger_validation_on_other_pane` | required |
| Renderer round-trips against Phase 13 corpus | 15.1 | `render_round_trip_corpus_*` (one per batch) | required |
| Validation endpoint returns 422 with pane and line/column | 15.2 | `validate_caddyfile_returns_422_with_pane_caddyfile_and_line_column` | required |
| Validation endpoint returns 422 with pane and JSON path | 15.2 | `validate_caddy_json_returns_422_with_pane_caddy_json_and_path` | required |

## Phase 15 exit checklist

- [ ] Every slice from 15.1 through 15.6 has shipped and its acceptance command passes.
- [ ] `just check` passes locally and in continuous integration.
- [ ] An invalid edit on either side shows a structured error pointing to the offending line and key.
- [ ] Apply is disabled while validation is failing.
- [ ] A valid edit produces a preview diff against current desired state before the user commits.
- [ ] The Caddyfile pane's legible form is documented as a Trilithon-managed rendering, not a verbatim Caddyfile.

## Cross-cutting test discipline

The Vitest corpus required by the phase reference uses these conventions, applied uniformly across every slice:

- **Fake timers.** Tests use `vi.useFakeTimers()` and `vi.advanceTimersByTime(200)` to deterministically traverse the debounce window. Wall-clock waiting is forbidden.
- **Stubbed validation adapter.** The `runValidate` function is injected through React context. The stub records every call and returns a programmable `Promise`. No test makes a real HTTP request.
- **Structured error fixtures.** `ValidationError` fixtures include realistic line/column or JSON-pointer paths so that error-rendering tests assert visible markers, not just text content.
- **Accessibility checks.** At least one test in the corpus runs `axe-core` against the rendered editor and asserts zero serious violations.
- **No `any`, no non-null assertions.** Per the project conventions; lint rules enforce.

## Open questions

1. The cross-pane re-render path for the Caddyfile pane currently calls a server endpoint (`POST /api/v1/desired-state/render-caddyfile`). Whether to ship a WASM build of the Phase 13 renderer for browser-side rendering is unresolved; the server call adds round-trip latency under the 200 ms debounce window in practice but warrants measurement.
2. The minimal client-side syntax highlighting (slice 15.4) is intentionally lo-fi. Whether a Monaco-class editor lands in V1.1 alongside the Tauri desktop wrap is filed for V1.1 planning.
3. The canonical directive ordering used by the renderer (slice 15.1, step 2.iv) is documented inline in the function body rather than in a separate ADR. Whether to elevate the ordering rule to its own ADR is filed for review during the Phase 16 documentation pass.
4. The validation endpoint runs full validation (lex, parse, expand, translate, validate) on every debounce tick. For very large desired states (approaching the Phase 13 size guards), this may exceed the 200 millisecond debounce window. Whether to add an incremental-validation path is unresolved and is deferred to V1.1.

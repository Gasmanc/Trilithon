# Contract Invariants

Human-curated invariants for the contracts listed in `contracts.md`. Never auto-generated. Normally merged by git (unlike `contracts.md` which uses `merge=ours`).

## Rules

- One section per contract symbol.
- Every symbol cited here MUST exist in `contracts.md` (enforced by `xtask invariant-check`, blocking).
- When a contract is renamed/removed, the corresponding section here must be moved or deleted in the same phase. The blocking `invariant-check` xtask will fail otherwise.
- `/plan-adversarial` reads this file as ground truth for what the system promises.

## Format

```markdown
## `<crate>::<path>::<symbol>`

**Invariants:**
- <invariant 1 — one sentence, in present tense>
- <invariant 2>

**Counter-examples (must NOT hold):**
- <case that this contract explicitly does not promise>

**Last reviewed:** <YYYY-MM-DD> by <user>
```

## Symbols

<!-- Example:

## `my_app::auth::verify_token`

**Invariants:**
- Returns `Err(AuthError::Expired)` for any token whose exp claim is in the past at the time of call.
- Never panics on malformed input — always returns `Err`.
- Idempotent: calling twice with the same input returns the same result (subject to clock progression for expiry).

**Counter-examples (must NOT hold):**
- Does not validate aud or iss claims — that is the caller's responsibility.

**Last reviewed:** 2026-05-08 by chris.carts
-->

_No invariants documented yet. Add entries as contracts are added to `contracts.md`._

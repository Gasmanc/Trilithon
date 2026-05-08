# Foundation 2 — Seam Registry

Enumerated list of architectural seams. Each seam is a boundary where one phase's outputs become another phase's inputs. Cross-phase integration tests live in `tests/cross_phase/<id>.rs` and exercise the contracts named in each seam entry.

## Rules

- `/tag-phase` MUST match identified seams against this list.
- `/tag-phase` cannot invent free-text names — proposed seams go to `seams-proposed.md` (staging).
- `/phase-merge-review` ratifies proposed seams into this file before merge can land.
- Removing a seam from this file requires a `/phase-merge-review` finding documenting why the boundary no longer exists.
- Renaming a seam is **not** allowed — create a new entry, mark the old one `superseded`, link them.

## Schema

```yaml
seams:
  - id: <slug>                        # stable, kebab-case, never renamed
    name: "<human-readable name>"
    contracts:                        # symbols from contracts.md exercised at this seam
      - <crate>::<path>::<symbol>
    test_file: tests/cross_phase/<slug>.rs
    introduced_in_phase: <N>
    status: active | superseded
    superseded_by: <id>               # optional, only if status=superseded
    notes: "<one-line>"
```

## Seams

```yaml
seams: []
# Example:
# seams:
#   - id: auth-storage
#     name: "Auth ↔ Storage"
#     contracts:
#       - my_app::auth::Session
#       - my_app::storage::SessionStore
#       - my_app::storage::SessionStore::insert
#       - my_app::storage::SessionStore::lookup
#     test_file: tests/cross_phase/auth_storage.rs
#     introduced_in_phase: 6
#     status: active
#     notes: "Session lifecycle from auth handler through storage."
```

## Test File Template

Each seam test file MUST contain stubs structured as:

```rust
//! Seam test: <seam-id>
//!
//! Contracts under test (mirror seams.md):
//!   - <contract 1>
//!   - <contract 2>

mod <seam_id_snake>_seam {
    use super::*;

    /// Contract: <contract 1>
    /// Required: at least one realistic input + one assert macro.
    #[test]
    fn upholds_<contract_1_kebab>_invariant() {
        // Arrange
        let input = /* realistic input */;
        // Act
        let result = /* call contract 1 */;
        // Assert (REQUIRED — must contain at least one assert macro)
        assert!(/* property */);
    }

    /// Contract: <contract 2>
    #[test]
    fn upholds_<contract_2_kebab>_invariant() {
        // ...
        assert_eq!(/* expected */, /* actual */);
    }

    /// Composition: contract 1 + contract 2
    #[test]
    fn composition_holds_under_realistic_workflow() {
        // ...
        assert!(/* end-to-end property */);
    }
}
```

Empty stubs (no assert macros) fail `xtask audit-duplicates --check-seam-stubs` and `scope-guardian-reviewer`.

## Lifecycle

| Action | Required steps |
|---|---|
| Add a seam | `/tag-phase` writes to `seams-proposed.md` → `/phase-merge-review` ratifies → entry moved here |
| Modify contracts on a seam | Update entry; corresponding test file must be updated in the same phase |
| Supersede | Mark `status: superseded`, link `superseded_by`. Keep entry — never delete |
| Delete | Only by explicit `/phase-merge-review` finding; the test file must also be removed |

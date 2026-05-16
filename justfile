# Trilithon — root justfile
# `just check` is THE gate. Everything else fans out from here.

# Default: list available targets
default:
    @just --list

# Run all linters + tests across every component
check:  check-rust check-typescript check-swift
    @echo "✓ all checks passed"

# Per-slice gate: run only the check target(s) for dirs modified since last commit.
# Uses `git diff HEAD` (staged + unstaged) — correct at gate time before commit.
# Falls back to full `just check` if changes span multiple dirs or touch root config.
# Do NOT use this as a substitute for `just check` at end-of-phase or in CI.
check-changed:
    #!/usr/bin/env bash
    set -euo pipefail
    changed=$(git diff HEAD --name-only 2>/dev/null)
    if [ -z "$changed" ]; then
        echo "check-changed: no changes detected — running full check"
        just check
        exit 0
    fi
    in_core=$(echo "$changed" | grep -q '^core/' && echo 1 || echo 0)
    in_web=$(echo "$changed" | grep -q '^web/' && echo 1 || echo 0)
    in_app=$(echo "$changed" | grep -q '^app/' && echo 1 || echo 0)
    # Root-level or multi-dir changes → full check
    has_root=$(echo "$changed" | grep -qv '^\(core\|web\|app\)/' && echo 1 || echo 0)
    dir_count=$(( in_core + in_web + in_app ))
    if [ "$has_root" -eq 1 ] || [ "$dir_count" -gt 1 ]; then
        echo "check-changed: root/multi-dir changes — running full check"
        just check
        exit 0
    fi
    [ "$in_core" -eq 1 ] && just check-rust
    [ "$in_web"  -eq 1 ] && just check-typescript
    [ "$in_app"  -eq 1 ] && just check-swift
    echo "✓ check-changed passed"

# Run all tests
test:  test-rust test-typescript test-swift
    @echo "✓ all tests passed"

# Format all code
fmt:  fmt-rust fmt-typescript fmt-swift
    @echo "✓ formatted"

# Auto-fix linter findings where possible
fix:  fix-rust fix-typescript fix-swift
    @echo "✓ fixed"


# --- rust (workspace-three-layer) ---
check-rust:
    cd core && cargo fmt --check
    cd core && cargo clippy --workspace --all-targets --all-features -- -D warnings
    cd core && cargo test --workspace --all-features

test-rust:
    cd core && cargo test --workspace --all-features

fmt-rust:
    cd core && cargo fmt --all

fix-rust:
    cd core && cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged
    cd core && cargo fmt --all

deny-rust:
    cd core && cargo deny check

# --- typescript (react-frontend) ---
check-typescript:
    cd web && pnpm install --frozen-lockfile || pnpm install
    cd web && pnpm typecheck
    cd web && pnpm lint
    cd web && pnpm format:check
    cd web && pnpm test --run

test-typescript:
    cd web && pnpm test --run

fmt-typescript:
    cd web && pnpm format

fix-typescript:
    cd web && pnpm lint:fix
    cd web && pnpm format

# --- swift (xcode-app) ---
check-swift:
    cd app && xcodegen generate --quiet || true
    cd app && swiftlint lint --strict
    cd app && swiftformat --lint .

test-swift:
    cd app && xcodebuild -project *.xcodeproj -scheme app test 2>&1 | xcbeautify || xcodebuild -project *.xcodeproj -scheme app test

fmt-swift:
    cd app && swiftformat .

fix-swift:
    cd app && swiftformat .
    cd app && swiftlint lint --fix

open-app:
    cd app && open *.xcodeproj


# ─── Cross-phase coherence (added by cross-phase-scaffold) ───
# Full reference: ~/.claude/templates/foundations/justfile-snippet

# Regenerate contracts.md from source. Run after touching contract-rooted symbols.
registry-regen:
    cd core && cargo xtask registry-extract --write
    cd core && cargo xtask invariant-check
    @echo "contracts.md regenerated. Review the diff before committing."

# Pre-merge gate — strict registry + advisory.
check-premerge:
    cd core && cargo check --workspace --all-targets
    cd core && cargo clippy --workspace --all-targets -- -D warnings
    cd core && cargo test --workspace
    -cd core && cargo test --workspace --test cross_phase || true
    -cargo machete || true
    -cd core && cargo xtask registry-verify || true
    -cd core && cargo xtask registry-check --strict || true
    -cd core && cargo xtask invariant-check || true
    -cd core && cargo xtask audit-duplicates || true
    -cd core && cargo xtask audit-duplicates --check-seam-stubs || true

# Migrate legacy review artefacts to Foundation 0 schema. One-time use.
migrate-findings *ARGS:
    cd core && cargo xtask migrate-findings {{ARGS}}

# Set the merge-review baseline. One-time use.
set-merge-review-baseline *ARGS:
    cd core && cargo xtask set-merge-review-baseline {{ARGS}}

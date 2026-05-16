# Phase 24 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, docs/architecture/architecture.md, docs/architecture/trait-signatures.md, docs/planning/PRD.md, docs/adr/0011-loopback-only-by-default-with-explicit-opt-in-for-remote-access.md, docs/adr/0014-secrets-encrypted-at-rest-with-keychain-master-key.md (ADR-0010 referenced via index), docs/todo/phase-24-systemd-deployment.md, docs/architecture/seams.md, docs/architecture/contract-roots.toml, docs/architecture/contracts.md
**Slices analysed:** 9

## Proposed Tags

### 24.1: Systemd unit file plus `tmpfiles.d` snippet and Caddy drop-in
**Proposed tag:** [trivial]
**Reasoning:** Four static, verbatim-specified deployment artefacts (unit file, `tmpfiles.d` snippet, Caddy drop-in, example config) under `deploy/systemd/`. No Rust code, no crate, no trait, no I/O performed by the slice itself. The `daemon.started` tracing event mentioned in the slice is emitted by the existing daemon (Phase 1+), not introduced here; the unit merely configures `Type=notify`. References ADR-0011/ADR-0014/PRD T2.7 but only as posture context, not as new contract surface.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 24.2: OS detection plus `detect_caddy` and `install_caddy_apt_repo` install-script functions
**Proposed tag:** [trivial]
**Reasoning:** Adds three bash functions to a single new file `deploy/systemd/install.sh`. One file, one "module", no Rust crate, no trait, no cross-layer dependency. Reads `/etc/os-release` and shells out to `caddy`/`apt-get` — but this is host-side install scripting, entirely outside the three-layer Rust workspace; it emits no audit rows and no tracing events others depend on.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 24.3: `create_trilithon_user`, `install_binary`, `seed_config` install-script functions
**Proposed tag:** [trivial]
**Reasoning:** Extends the single `install.sh` file with three more bash functions that provision the system user, install the binary with SHA-256 verification, and seed config directories. Still one file, no Rust code, no trait, no audit/tracing convention. ADR-0014 is cited only to fix directory modes for the secrets path; the slice does not touch the secrets vault implementation.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 24.4: `start_service`, `verify_running`, `rollback_partial`, install entry point
**Proposed tag:** [standard]
**Reasoning:** Completes `install.sh` (single file) with the wiring, verification, rollback, and `main` entry point — still one file, no Rust code, no trait. Lifted above trivial because it composes the whole install flow into a stateful host-mutating procedure with a rollback path, drops files into system-wide systemd locations, and is the dependency root for slices 24.5–24.9. It does not itself add Rust I/O or cross a Rust layer boundary; the audit kinds and tracing events listed are emitted by the already-built daemon, not by this slice.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 24.5: Uninstall script (six functions)
**Proposed tag:** [standard]
**Reasoning:** One new self-contained bash file `deploy/systemd/uninstall.sh` with six functions. Single file and no Rust/trait/layer involvement keeps it below cross-cutting; placed at standard rather than trivial because it is a stateful host-mutating procedure (service teardown, user/group deletion, optional data removal) that is the symmetric counterpart of the 24.4 install flow and is depended on by 24.7 and 24.8.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 24.6: CI hardening-score gate (`systemd-analyze security`)
**Proposed tag:** [trivial]
**Reasoning:** One new GitHub Actions workflow file `.github/workflows/systemd-hardening.yml`, specified verbatim. No Rust code, no crate, no trait, no audit/tracing convention. It is CI infrastructure that scores the already-authored unit file; it introduces no shared convention other slices follow.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 24.7: CI smoke matrix on Ubuntu 24.04 and Debian 12 plus negative tests
**Proposed tag:** [standard]
**Reasoning:** One CI workflow plus three test scripts under `deploy/systemd/test/`, all verbatim-specified. No Rust code, no trait, no layer boundary, so not cross-cutting. Above trivial because it spans several new files forming a coherent test harness (positive matrix plus two negative-path scripts) and exercises the full install/verify/uninstall cycle that 24.4 and 24.5 deliver.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 24.8: `.deb` package build with postinst/prerm/postrm hooks
**Proposed tag:** [standard]
**Reasoning:** Touches `core/Cargo.toml` (a `[package.metadata.deb]` block — packaging metadata only, not code, not a dependency edge), one new CI workflow, and three Debian maintainer scripts. The `Cargo.toml` edit adds no crate dependency and crosses no Rust layer boundary; the maintainer scripts duplicate install/uninstall logic in bash. It is multi-file but the manifest change is inert metadata, so it stays standard rather than cross-cutting.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** The `core/Cargo.toml` edit is the only workspace-manifest touch in the phase; if a reviewer treats any manifest change as cross-cutting it would lift to cross-cutting, but the change adds no dependency and no architectural edge.

### 24.9: Operator and end-user documentation
**Proposed tag:** [trivial]
**Reasoning:** Two new Markdown documents plus one heading-lint bash script under `deploy/systemd/`. Pure documentation and a trivial lint; no Rust code, no trait, no I/O, no audit/tracing convention. Wiring the lint into `just check` follows the existing Phase 23 §23.9 pattern rather than introducing a new one.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

## Summary
- 4 trivial / 5 standard / 0 cross-cutting / 0 low-confidence

(One slice — 24.8 — carries medium confidence; the rest are high. No slice is cross-cutting.)

## Notes

- Phase 24 is entirely a deployment/packaging phase. Every slice lives under
  `deploy/systemd/` or `.github/workflows/`, except the inert
  `[package.metadata.deb]` block appended to `core/Cargo.toml` in 24.8.
- No slice implements or extends a Rust trait. `trait-signatures.md` was
  consulted because the phase reference cites `core::secrets::SecretsVault`,
  but only to confirm the hardened systemd unit accommodates the keychain
  backend's `Secret Service` D-Bus connection over `AF_UNIX` (already allowed
  by `RestrictAddressFamilies`). No trait surface is touched.
- No slice crosses a `core ↔ adapters ↔ cli` layer boundary. Shell scripts and
  systemd units sit outside the Rust workspace entirely.
- The seam registry (`seams.md`) contains only Phase 7 apply-path seams; no
  Phase 24 slice exercises or proposes a seam. `seams-proposed.md` needs no new
  entries from this phase.
- The contract registry is empty and `contract-roots.toml` lists only Phase 7
  apply-path roots. Phase 24 adds no contract symbols (no `pub` Rust surface).
- Audit kinds and tracing events listed in slice "emitted" sections
  (`daemon.started`, `config.applied`, `auth.bootstrap-credentials-rotated`,
  etc.) are emitted by the pre-existing daemon during install verification, not
  introduced by Phase 24. The §6.6 and §12.1 vocabularies require no update.
- ADR references per slice stay at or below two distinct ADRs except where the
  phase header aggregates ADR-0010/0011/0014; individual slices cite at most
  ADR-0011 + ADR-0014. The "3+ ADRs" cross-cutting trigger is not met because
  the citations are deployment-posture context, not new architectural decisions.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.

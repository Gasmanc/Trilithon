# ADR-0005: Ship the web UI first; defer Tauri desktop wrap to V1.1

## Status

Accepted — 2026-04-30.

## Context

Trilithon's user-facing surface can be delivered as a browser-loaded web
application served by the daemon, as a native desktop application that
embeds the same web assets through Tauri, or as both. The binding
prompt (section 2, item 5) fixes the sequencing: the web UI ships first,
and the Tauri desktop wrap is V1.1 work, explicitly out of scope for
V1.

Forces:

1. **The web UI is a hard requirement of the V1 deliverable** (T1.13).
   Headless deployments (a daemon on a home server, accessed from a
   laptop on the same network) need a browser-accessible interface
   regardless of whether a desktop application also exists.
2. **The Tauri wrap is a packaging change, not a feature change.** A
   correctly architected web UI served on loopback is the input that
   Tauri 2.x consumes. Building Tauri first would require building the
   web UI anyway and would add platform-specific build tooling
   (Xcode toolchain, Windows MSVC, Linux WebKit2GTK) to the V1 path.
3. **Deferred desktop work has near-zero retrofit cost when the web
   UI is built correctly.** ADR-0004 selects Vite, whose static
   build output Tauri consumes directly. ADR-0011 binds the daemon to
   loopback, which is what Tauri's embedded webview connects to. No
   architectural decision in V1 precludes V1.1 desktop packaging.
4. **Native packaging adds release-engineering burden.** Code signing
   on macOS and Windows, notarisation on macOS, auto-update channels,
   and per-platform installer pipelines are real engineering work.
   Doing this concurrently with V1 feature delivery would slow V1.
5. **Headless users are an explicit target.** Tier 2 deployment paths
   (T2.7 systemd, T2.8 Docker Compose) presume the user reaches the
   UI through a browser, often from a different machine. A
   desktop-only product would not serve this audience.

## Decision

V1 SHALL deliver the web UI as the sole user interface. The Trilithon
daemon SHALL serve the React-built static assets on `127.0.0.1:<port>`
(default 7878) per T1.13 and ADR-0011.

V1 SHALL NOT include a Tauri build, a `src-tauri/` directory in the
repository's primary tree, native installers, code-signing infrastructure,
or auto-update machinery. References to a desktop application in V1
specifications SHALL be marked "OUT OF SCOPE FOR V1" per binding prompt
section 0.

V1.1 SHALL introduce the Tauri 2.x desktop wrap. The V1.1 work SHALL
reuse the unmodified web UI bundle. V1.1 SHALL NOT require schema
changes, mutation-API changes, or daemon-protocol changes. If V1.1
discovers a needed change in the daemon protocol, that change SHALL
land first as a backwards-compatible web-UI-driven change with its own
ADR.

V1 architecture SHALL preserve V1.1 viability:

- The web UI SHALL avoid browser APIs unavailable in WebKit2GTK
  (Linux Tauri target) and the platform-default webview engines on
  macOS and Windows.
- The web UI SHALL NOT rely on cookies that require a public-suffix-list
  domain; loopback origins must work.
- The daemon SHALL accept connections from the Tauri webview's origin
  (`tauri://localhost` or equivalent) when V1.1 ships, with the
  authentication posture from T1.14 and ADR-0011 preserved.

## Consequences

**Positive.**

- V1's release-engineering surface is small: a Rust binary and a
  static asset bundle. There is no code-signing certificate to
  acquire, no notarisation pipeline to maintain, and no per-platform
  installer matrix to test.
- The web-first delivery serves headless users out of the box, which
  matches the home-lab and small-business deployment targets that
  motivate Trilithon.
- The team's V1 attention stays on the control-plane logic (T1.1
  through T1.15, T2.1 through T2.12) where the engineering risk
  lives.

**Negative.**

- Users on macOS and Windows who expect a Dock or Start Menu icon
  will install Trilithon as a daemon plus a browser tab, which is
  less polished than a native application would feel. V1.1 closes
  this gap; V1 lives with it.
- The first impression for casual desktop users is "open a browser
  to localhost," which is unfamiliar to non-technical users. The
  Tier 2 deployment paths (T2.7, T2.8) make this less of a paper-cut
  for the V1 target audience but do not eliminate it.
- Browser-based delivery makes some integrations (system notifications,
  file system access for backup export) harder than they would be in
  Tauri. V1 SHALL solve these through web-platform APIs (the Web
  Notifications API, the File System Access API) where possible and
  SHALL accept friction where it is not.

**Neutral.**

- The web UI SHALL be the canonical interface even after V1.1 ships.
  Tauri is a packaging layer, not a divergence.
- Open question for V1.1: whether the Tauri build embeds a copy of
  the daemon binary or connects to a separately installed daemon.
  This is recorded as an open question in the V1.1 phase plan, not
  in this ADR.

## Alternatives considered

**Tauri-first with web access secondary.** Build the desktop
application first and treat browser access as a fallback. Rejected
because headless deployments are an explicit V1 target (T2.7, T2.8)
and would be poorly served by a desktop-first build, and because the
desktop-build-engineering tax would slow V1 feature delivery.

**Both at once.** Ship V1 with both a web UI and a Tauri desktop
application. Rejected because the desktop application's release
engineering (signing, notarisation, installer pipelines) is real
work that competes with V1 feature delivery, and because deferring
it costs nothing as long as V1's architecture stays Tauri-compatible
(which this ADR ensures).

**Electron instead of Tauri for V1.1.** Use Electron for cross-platform
packaging. Rejected at the V1.1 sketch level because Electron bundles
a Chromium runtime, producing 100-200 MB installers; Tauri uses the
platform webview, producing 10-20 MB installers, which is more in
keeping with Trilithon's local-first ethos.

**Progressive Web App (PWA) installation.** Have users "install" the
web UI through the browser's PWA flow. Rejected because PWA support
is uneven across platforms, because PWA does not provide the
filesystem and notification access a native wrap can, and because it
does not eliminate the desktop-installer mental model for users who
expect one.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#2-non-negotiable-constraints`,
  item 5; section 4 feature T1.13.
- ADR-0004 (React, TypeScript, Tailwind, Vite frontend stack).
- ADR-0010 (Two-container deployment with unmodified official Caddy).
- ADR-0011 (Loopback-only by default with explicit opt-in for remote
  access).

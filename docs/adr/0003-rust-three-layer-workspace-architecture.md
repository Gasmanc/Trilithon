# ADR-0003: Adopt a three-layer Rust workspace architecture

## Status

Accepted — 2026-04-30.

## Context

Trilithon's backend is Rust (binding prompt section 2, item 4). The
project layout already follows a three-crate workspace under `core/`:
`crates/core`, `crates/adapters`, and `crates/cli`. This ADR records
the rule that protects the boundary, the rationale, and the operational
consequences.

The control plane mediates between an in-memory typed model of Caddy
configuration and several outside-world systems: Caddy's admin endpoint
(HTTP over Unix socket), SQLite, the Docker socket, the operating
system keychain, the filesystem, the wall clock, and the network. Each
of these is a source of test-time pain when entangled with pure logic.

Forces:

1. **The mutation pipeline is dense logic.** Validation, diffing,
   content addressing, redaction, and version arithmetic are pure
   functions of inputs. Pure functions are cheap to test, easy to
   reason about, and immune to flakiness.
2. **Outside-world calls are slow and conditional.** Caddy may be
   unreachable. SQLite may be locked. Docker may be absent. Keeping
   these calls behind a thin adapter layer means the mutation pipeline
   can be exercised at speed in unit tests with no test doubles in
   production paths (constraint 8).
3. **The compiler enforces the rule at zero ongoing cost.** A
   manifest-level dependency restriction (`core` does not depend on
   `tokio`, `reqwest`, `rusqlite`, or anything async or I/O-bearing)
   produces a hard error if a contributor accidentally couples logic
   to runtime concerns.
4. **The project's `CLAUDE.md` already encodes the rule.** The repo
   convention says "If you need a cross-layer dep, stop and ask." The
   ADR formalises why and how.

## Decision

Trilithon's Rust workspace SHALL contain three crates with the
following responsibilities and dependency rules:

**`crates/core`.** Pure logic. The desired-state model, the typed
mutation set (T1.6), validation, diffing, canonical JSON
serialisation, content addressing, secrets-aware redaction (T1.7,
T1.15, hazard H10), policy preset definitions (T2.2), capability
descriptors (T1.11), and the optimistic-concurrency version arithmetic
(T2.10) live here. `core` MUST NOT depend on `tokio`, any HTTP client,
any database driver, any filesystem-touching crate, any environment
or process-level crate, or any foreign-function-interface crate. `core`
SHALL declare `#![forbid(unsafe_code)]` at the crate root.

**`crates/adapters`.** Wraps the outside world. The Caddy admin client
(via `hyper`, `hyperlocal`, or `reqwest` over a Unix socket — see
ADR-0011), the SQLite repository (via `rusqlite` or `sqlx` with the
SQLite backend — see ADR-0006), the Docker discovery client, the
keychain accessor (see ADR-0014), the filesystem snapshot store, and
the wall-clock and monotonic-clock providers live here. `adapters`
SHALL depend on `core` and SHALL NOT depend on `cli`. `adapters` SHALL
declare `#![forbid(unsafe_code)]` at the crate root.

**`crates/cli`** (or `crates/daemon` when renamed). The binary.
Argument parsing (`clap` derive), tracing subscriber initialisation,
signal handling, the HTTP server that delivers the web UI (T1.13)
and the typed tool gateway (T1.6, ADR-0008), and process-level
composition root. `cli` SHALL depend on `core` and `adapters`. `cli`
is the only crate permitted to use `anyhow::Result` and the only
crate where panicking on impossible startup states is acceptable
(though the constraint against `unwrap()`/`expect()`/`panic!` in
production paths still applies — see constraint 7 and ADR-0014's
fallback handling).

Cross-layer rules:

- A pull request that adds a forbidden dependency to `core` or
  `adapters` SHALL be rejected.
- A type defined in `core` MUST NOT contain an I/O handle, a database
  connection, a socket, a `tokio::sync` primitive, or a
  reference-counted handle to any of the above.
- An `adapters` function that performs I/O SHALL return a typed error
  defined either in `core` or in `adapters`, never `anyhow::Error`.
- A `cli` function MAY use `anyhow::Result` for top-level composition.

`just check` SHALL run `cargo clippy --workspace --all-targets
--all-features -- -D warnings` and SHALL fail on any warning.

## Consequences

**Positive.**

- The mutation pipeline (T1.1, T1.6) is exercised in unit tests
  without spawning a Tokio runtime, opening a SQLite database, or
  contacting Caddy.
- The "no mocks in production paths" constraint (constraint 8)
  becomes structurally easy: production code in `core` has no I/O
  to mock, and `adapters` is constructed once at startup with real
  implementations.
- The boundary catches design errors. A reviewer who sees an attempt
  to add `tokio` to `core/Cargo.toml` knows immediately that the
  proposed change is mismodelled.

**Negative.**

- Some operations that feel atomic to a beginner ("validate this
  mutation against running Caddy state") must be split: pure
  validation in `core`, capability lookup in `adapters`, composition
  in `cli`. The split is the point, but it has a learning curve.
- Trait abstractions in `core` describing what `adapters` provides
  must be designed carefully. Premature generalisation (constraint
  on three-uses-before-extracting) applies; the project's
  `CLAUDE.md` already enforces it.

**Neutral.**

- Crate compile times are dominated by `adapters` and `cli`. `core`
  is fast. This shapes development feedback in a way that rewards
  iterating on logic without bouncing through I/O changes.
- The `Storage` trait that fronts SQLite (T1.1, hazard H14) lives in
  `core` and is implemented in `adapters`. This positions ADR-0006's
  forward-compatibility for PostgreSQL.

## Alternatives considered

**Single-crate binary.** Put everything in one crate with internal
modules. Rejected because module-level boundaries are advisory and
do not enforce dependency restrictions; nothing prevents a logic
module from importing `tokio` once the dependency is in `Cargo.toml`.

**Two-layer split (logic and I/O).** Merge `cli` into `adapters`.
Rejected because the binary's composition logic (argument parsing,
signals, top-level tracing) belongs in a layer that is allowed to
use `anyhow::Result`, while the I/O wrappers should produce typed
errors. Conflating them would force `anyhow` into the I/O layer or
typed errors into the binary — both worse than the three-layer
split.

**Hexagonal / ports-and-adapters with many small crates.** Split
each external system into its own crate (a `caddy-client` crate, a
`sqlite-store` crate, a `docker-discovery` crate). Rejected for
V1 because the three-uses rule (project `CLAUDE.md`) discourages
abstraction before pressure exists. The split may emerge later if
the `adapters` crate becomes unwieldy.

**Onion architecture with domain events.** Introduce a domain-event
bus and let layers publish/subscribe. Rejected because it adds a
runtime concept (event ordering, delivery guarantees) to V1 that is
not justified by the requirements; the typed mutation API (T1.6) is
already a sufficient choke point.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#2-non-negotiable-constraints`,
  items 4, 7, 8.
- Project `CLAUDE.md` Rust conventions section.
- ADR-0006 (SQLite as V1 persistence layer).
- ADR-0008 (Bounded typed tool gateway for language models).
- ADR-0014 (Secrets encrypted at rest with keychain master key).

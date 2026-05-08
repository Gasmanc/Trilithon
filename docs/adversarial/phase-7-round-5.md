# Adversarial Review — Phase 7 — Round 5

**Prior rounds:** R1 (F001–F010), R2 (F011–F019), R3 (F020–F026), R4 (F027–F031). All unaddressed.

---

## Summary

Round 5 probed six specific surfaces. Two surfaces (notes-column sizing, `JsonPointer` Ord collision) failed to yield a finding on careful examination. One surface (fake-CaddyClient masking normalisation) overlaps materially with the already-raised F014. Three concrete new findings were constructed: async timing in the property test, unspecified `AuditWriter` backpressure semantics, and absent `caddy_instance_id` scoping on the `rollback()` snapshot lookup.

---

## Findings

### F032 — TLS observer writes terminal row asynchronously; `correlation_id` property-test invariant is timing-dependent
**Severity:** MEDIUM
**Category:** composition-failure
**Slice:** 7.7 / 7.8

**Attack:** The Slice 7.7 property test runs N=200 scenarios and asserts "exactly one terminal row per `correlation_id`" immediately after each synchronous `apply()` returns. Any TLS-issuing scenario spawns a detached `TlsIssuanceObserver` task. That task writes its row after a polling delay (up to 120 s). On fast local runs the assertion fires before the observer row lands — the invariant appears to hold. On a slow CI environment or when the test process stays alive between cases (e.g., `cargo nextest`), the observer from scenario N fires during scenario N+1's assertion window, writing an unexpected row and causing a spurious failure — or worse, the observer fires after the test is declared passing and corrupts the row count for subsequent scenarios.

**Why the design doesn't handle it:** The design assumes the property test can synchronously assert the terminal-row invariant while a detached async task is still potentially in-flight against the same database.

**Blast radius:** The property test produces false positives in fast environments and intermittent failures in slow environments. The TLS terminal-row path is never reliably exercised. The "exactly one terminal row" guarantee is not verified for TLS completion cases.

**Recommended mitigation:** Inject a mock observer that records its intended write synchronously during the property test; separately add an integration test that awaits the observer via a test-visible `JoinSet` before asserting. This also aligns with the F024 mitigation (register observer handles with the shutdown `JoinSet`).

---

### F033 — `AuditWriter` bounded-channel backpressure semantics unspecified; blocking or dropping both produce concrete SLA or audit failures
**Severity:** HIGH
**Category:** assumption-violation
**Slice:** 7.4 / 7.7

**Attack:** `AuditWriter` holds a bounded MPSC channel (architecture §9). Under I/O pressure (WAL checkpoint, fsync stall, disk-full), the writer task slows and the channel fills. `AuditWriter::record` must do one of: (a) block — the apply coroutine cannot return `ApplyOutcome::Succeeded` until capacity is available, stalling the caller and violating the p95 < 2s latency SLA when 50+ concurrent applies are queued; (b) drop — the apply returns `Succeeded` but no `config.applied` row is written, violating the single-terminal-row invariant and ADR-0009; (c) return a typed error — but F011 (unaddressed) shows the success-leg audit failure path is itself unspecified.

**Why the design doesn't handle it:** The design specifies neither the channel capacity nor the backpressure behaviour of `AuditWriter::record`. The success leg of `apply()` treats the audit write as non-failing.

**Blast radius:** Under I/O pressure: either the latency SLA is violated (blocking) or the audit log is silently incomplete (dropping). Neither consequence is observable until a production incident. The single-terminal-row invariant the property test is meant to verify cannot be trusted under real I/O conditions.

**Recommended mitigation:** Specify `AuditWriter::record` uses `try_send` and returns `AuditWriteError::ChannelFull` on backpressure. The caller (`CaddyApplier`) must treat this as `ApplyError::Storage` and return `ApplyOutcome::Failed` — not silently succeed without an audit row. Specify the channel capacity (e.g., 256 entries) in the design. Add a bounded-channel-full integration test asserting `ApplyOutcome::Failed` is returned when the channel is saturated.

---

### F034 — `rollback()` accepts any `SnapshotId` without `caddy_instance_id` scoping; cross-instance snapshot applied silently
**Severity:** HIGH
**Category:** abuse-case
**Slice:** 7.4

**Attack:** The `Applier::rollback(&target: &SnapshotId)` method takes a `SnapshotId` that is a 64-character SHA-256 hex string with no embedded `caddy_instance_id`. The rollback implementation is expected to query `SELECT ... FROM snapshots WHERE snapshot_id = ?` without a `caddy_instance_id = ?` predicate. A caller — including a language-model agent via the typed tool gateway (ADR-0008) — that has access to a foreign instance's `snapshot_id` (from a shared export bundle, a copied database, or `config show` output) can supply it to `rollback()`. The applier loads the foreign snapshot's `desired_state_json`, re-renders it stamping the current instance's sentinel, passes the optimistic-concurrency check if `config_version` matches, and POSTs the foreign instance's routes and upstreams to Caddy. Caddy now serves a completely different set of routes with no error raised.

**Why the design doesn't handle it:** The design assumes callers supply only snapshot IDs from the current instance's lineage. There is no type-level or query-level enforcement of this. The `config.rolled-back` audit row is written referencing a snapshot that was never part of this instance's history.

**Blast radius:** The current Caddy instance silently begins serving a foreign configuration. The audit trail references a snapshot from a different lineage. ADR-0009's rollback semantics ("set the desired-state pointer to an existing snapshot") are satisfied in letter but not spirit. This is exploitable by the LLM tool gateway with only a valid `expected_version`.

**Recommended mitigation:** The rollback storage query MUST filter by `caddy_instance_id`: `SELECT ... FROM snapshots WHERE snapshot_id = ? AND caddy_instance_id = ?`. Return a typed `ApplyError::SnapshotNotInLineage` if the snapshot exists but belongs to a different instance. Add a test constructing two storage instances with different `instance_id` values that asserts the foreign snapshot is rejected.

---

## Surfaces with no finding

**`ApplyAuditNotes` column size:** All fields are intrinsically bounded by their types. The total serialised notes JSON is capped well under 1 KiB. No finding.

**`JsonPointer` lexicographic `Ord` and duplicate paths:** RFC 6901 `/foo/bar` and `/foo/bar/` address different locations (the latter references an empty-string key inside `bar`). No scenario was constructed where two distinct RFC 6901 pointers in the `BTreeMap` resolve to the same Caddy location and produce a silently-accepted malformed document. No finding.

**Fake `CaddyClient` masking real Caddy normalisation:** The test double trivially passes the equivalence check by echo-returning what was POSTed. This overlaps substantially with F014 (DiffEngine ignore-list undefined). No new independent finding.

---

## Severity summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 2 (F033, F034) |
| MEDIUM   | 1 (F032) |
| LOW      | 0 |

---

## Round 5 verdict

**The design has been sufficiently probed.** Five rounds across 13 failure categories have produced 34 findings. The finding rate has dropped to 0 critical, 2 high in this round. No major untouched surfaces remain.

**Must address before implementation:** F034 (add `caddy_instance_id` scoping to the rollback snapshot query — exploitable by the LLM gateway with minimal access) and F033 (specify `AuditWriter` backpressure contract). F032 can be addressed during test design.

**Proceeding to `--final` is appropriate** once F033 and F034 are incorporated into the slice specifications. The remaining 31 prior findings should be captured in the decision doc's unaddressed-findings table with explicit acceptance rationale or a mitigation commitment.

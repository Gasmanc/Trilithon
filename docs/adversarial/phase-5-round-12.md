# Adversarial Review — Phase 05 — Round 12

**Design summary:** A content-addressed, append-only SQLite snapshot store (WAL mode) for a Caddy reverse-proxy configuration daemon, using a three-layer Rust workspace. Writes are protected by OCC versioning, content-addressed deduplication with SHA-256, and immutability triggers. Reads are instance-scoped with offset-based pagination. A separate verification binary re-hashes stored rows against stored IDs.

**Prior rounds:** 11 prior rounds reviewed — all previously identified issues are marked as addressed. No prior findings are re-raised below.

---

## Findings

### [HIGH] Step-14 cross-instance "not found" arm returns `VersionRace` for an impossible state — masks genuine storage invariant violations

**Category:** Logic Flaws

**Trigger:** Step 13's plain INSERT fails with a UNIQUE violation on `id`. Step 14 runs the instance-scoped length check binding both `id` and `caddy_instance_id` — no row found for this instance. The cross-instance check `SELECT 1 FROM snapshots WHERE id = ? LIMIT 1` runs. The design's commentary says "if found: `VersionRace`; if not: `VersionRace`" — both branches return the same variant. The "if not found" sub-branch is logically impossible: if the UNIQUE violation fired on `id`, then some row with that `id` must exist in the table.

**Consequence:** If this arm ever fires — due to a concurrent DELETE (impossible under triggers, but possible if migrations are mis-applied or a bypass is used during maintenance), a race between the UNIQUE check and row deletion, or a future code path that inadvertently gets here — the impossible state is silently classified as `VersionRace`. The caller retries indefinitely. The storage invariant violation (INSERT failed, but no row with that `id` exists) is permanently masked. Future refactors that split the two arms to add distinct behavior will produce a latent bug that is only triggered by the impossible state path.

**Design assumption violated:** The design assumes both arms of the cross-instance check are semantically equivalent. They are not: "found cross-instance" is a known, handled race; "not found" is an impossible state that should be distinguished from retryable races.

**Suggested mitigation:** The "not found" arm should return a distinct error variant: `WriteError::InvariantViolated { message: "INSERT failed with UNIQUE violation on id, but no row with that id exists in the table" }`. This preserves the diagnostic signal and prevents future refactors from silently treating impossible states as retryable races. No change to the happy path.

---

### [MEDIUM] Step-0 runtime warn checks `max_desired_state_bytes` ceiling on every call — fires constantly for deployments with large max but small typical payloads

**Category:** Logic Flaws

**Trigger:** Step 0 runs the timeout formula against `self.max_desired_state_bytes` (the configured ceiling), not against the actual `bytes.len()` for the current call. An operator configures `with_limits(100_MB, 2s)` — the formula warns because 100 MiB * 1ms/KiB ≈ 100s > 2s. Every subsequent `write()` emits this warning regardless of whether the actual payload is 1 KB or 100 MB. For a deployment where most writes are small configs but the ceiling is generous, the warning fires on every single write.

**Consequence:** Warning fatigue — operators habituate to ignoring the constant noise. When a genuinely oversized payload arrives that would time out, the warning is indistinguishable from background noise. The step-0 check was intended to surface actionable problems; checking the ceiling instead of actual size makes it structurally unactionable on every call.

**Design assumption violated:** The step-0 warn was described as firing "on every write where limits appear undersized." In practice, "limits appear undersized" was evaluated against the configured ceiling, not against the actual write. These are different conditions.

**Suggested mitigation:** Move the configured-ceiling formula check to construction time — warn once in `SnapshotWriter::new()` if `DEFAULT_WRITE_TIMEOUT` appears undersized for `DEFAULT_MAX_DESIRED_STATE_BYTES`, or in `with_limits()` (already done). Step 0 should check the actual serialized payload size for the current call: if `bytes.len() > threshold_that_could_timeout(self.write_timeout)`, emit the warn. This way the warning fires only when the specific write in progress is at risk, not for every write in the process lifetime.

---

### [MEDIUM] `regen-snapshot-hashes` does not distinguish `version < CURRENT` (legacy) from `version > CURRENT` (binary outdated) — future-version rows silently pass as "skipped"

**Category:** Logic Flaws

**Trigger:** A future migration introduces `canonical_json_version = 2`. The `regen` binary is not updated and still has `CANONICAL_JSON_VERSION = 1`. The binary runs without `--strict`. All version-2 rows are emitted as "skipped (legacy version N)" and the binary exits zero — even though these rows are *newer* than the binary's knowledge, not older. The word "legacy" implies the binary knows more than the row; here it knows less.

**Consequence:** An operator running `regen` as a health check after a canonical JSON version migration gets a zero exit code and believes all stored hashes are verified, when a substantial fraction of rows (all written after the migration) were silently skipped. If those rows contain corrupted hashes due to a canonicalization bug in version 2, the corruption goes undetected. The `--strict` flag from R9 would catch this if operators use it, but its description is "exit non-zero if any rows at a legacy canonical_json_version are skipped" — which implies it guards against old rows, not the forward case.

**Design assumption violated:** The skip predicate `canonical_json_version != CANONICAL_JSON_VERSION` is symmetric — it does not distinguish past from future versions. The word "legacy" in the output and `--strict` documentation implies the design only considered the backward case.

**Suggested mitigation:** Change the skip classification: rows with `canonical_json_version < CANONICAL_JSON_VERSION` → "skipped (legacy version N, written by older binary)"; rows with `canonical_json_version > CANONICAL_JSON_VERSION` → emit `tracing::warn!("skipped (future version N, binary is outdated — upgrade regen binary before verifying these rows)")` AND exit non-zero regardless of `--strict`. A binary that cannot verify a row should not exit zero. Update the `--strict` documentation to clarify it covers both directions.

---

### [LOW] `in_range` result set across pages can silently skip rows under high write throughput with same-millisecond same-run-id writes

**Category:** Logic Flaws

**Trigger:** Two snapshots for the same `caddy_instance_id` and `daemon_run_id` are written within the same nanosecond (possible on coarse-resolution clock platforms), producing identical `(created_at_ms, created_at_monotonic_nanos, daemon_run_id)` keys but different `config_version` values. A caller fetches page N of `in_range`, receiving the row with `config_version=7` as the last entry. Between pages, a concurrent insert adds a row whose four-key sort position falls between `config_version=7` and `config_version=8`. The next page fetched with `OFFSET=page_size` skips `config_version=8` (now shifted to OFFSET+1). The documented offset instability caveat covers this, but no watermark or cursor is surfaced to the caller so they cannot detect the skip.

**Consequence:** Callers iterating `in_range` in a high-write scenario silently receive an incomplete result set across pages. Phase 7 consumers relying on `in_range` for history replay may miss a snapshot. The documented mitigation (keyset cursor) is deferred to Phase 7+.

**Design assumption violated:** The design documents the instability but provides no mechanism for callers to detect whether their paginated result set is complete.

**Suggested mitigation:** Return the `config_version` of the last row on each page as a `next_cursor` field in the `in_range` response type, alongside the row vec. Callers who care about completeness can use this to detect if a re-scan is needed (compare `next_cursor` from page N to the first row of page N+1). This does not implement full keyset pagination but gives callers a signal without requiring a protocol change. Document as Phase 7 pre-work.

---

## Summary

**Critical:** 0 &nbsp; **High:** 1 &nbsp; **Medium:** 2 &nbsp; **Low:** 1

**Top concern:** The step-14 cross-instance "not found" arm silently returns `VersionRace` for an impossible storage state — if it ever fires (maintenance bypass, mis-applied migration, future code path), the invariant violation is permanently masked and the caller retries indefinitely.

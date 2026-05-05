# Adversarial Review — Phase 05 — Round 10

**Design summary:** A content-addressed, append-only snapshot store backed by SQLite for a Caddy reverse-proxy configuration daemon. Snapshots are keyed by SHA-256 of canonical JSON, versioned monotonically per `caddy_instance_id`, and made permanently immutable by database-level triggers (RAISE ABORT on UPDATE and DELETE). A `regen-snapshot-hashes` binary is provided to rehash rows when the canonical JSON version is bumped.

**Prior rounds:** 9 prior rounds reviewed — all previously identified issues are marked as addressed in the design. No prior findings are re-raised below.

---

## Findings

### [CRITICAL] `regen-snapshot-hashes` cannot modify any snapshot row — immutability triggers block every code path available to it

**Category:** Logic Flaws

**Trigger:** Migration 0004 installs `snapshots_no_update` (BEFORE UPDATE RAISE ABORT) and `snapshots_no_delete` (BEFORE DELETE RAISE ABORT) on the `snapshots` table. When the canonical JSON version is bumped and `regen-snapshot-hashes` runs, it must change each row's `id` column (the SHA-256 hash, which is the PRIMARY KEY) to the new hash computed by the new algorithm. There is exactly one code path for changing the `id` of an existing row: `UPDATE snapshots SET id = new_hash … WHERE id = old_hash`. This fires `snapshots_no_update` and RAISE ABORT ends the statement. The only alternative is DELETE-then-INSERT, but `snapshots_no_delete` blocks DELETE with RAISE ABORT. A plain INSERT with the new hash leaves the old row intact (primary key conflict if old hash were reused — but it won't be; old and new hashes are different — so INSERT succeeds for the new hash but the old row persists). No mechanism described in the design bypasses either trigger.

**Consequence:** The `regen-snapshot-hashes` tool is structurally inoperable. Every attempt to update a snapshot row fails at the database level with a trigger abort. If the tool wraps all updates in a single transaction, the first RAISE ABORT rolls back the entire transaction. The operator is left with an inconsistent view (new version's binary, old version's hashes in the DB) and no recovery path. The version-bump workflow the binary is designed to support — canonicalize existing rows under the new algorithm, update their stored hashes — is architecturally impossible given the current trigger design.

**Design assumption violated:** The design assumes `regen-snapshot-hashes` can "update row hashes in a single transaction." This is incompatible with the ADR-0009 mandate that there is no `UPDATE snapshots` statement anywhere in the codebase, and with the trigger enforcement of that mandate. The design has not specified any mechanism (e.g., a trigger-disable path scoped to a privileged maintenance connection, a shadow-table migration strategy, or an INSERT-only rehash via a new table) to allow the tool to operate.

**Suggested mitigation:** Redesign the rehash workflow to be INSERT-only and verification-only (two distinct modes):
1. **Verification mode** (default): reads each row, recomputes the canonical hash, compares against the stored `id`. No writes — compatible with immutability triggers. This is the safe default that detects corruption without touching the DB.
2. **Rehash mode** (opt-in): intended for use after a `CANONICAL_JSON_VERSION` bump. Since rows are immutable, the correct approach is: for each row at the old version, write a *new* snapshot row via `SnapshotWriter::write` with the same content but the current canonical version. The old row is retained (append-only). The new row gets a new `id` (new hash) and a new `config_version`. This requires an ADR-ratified decision on whether cross-version dedup is acceptable and how parent pointers are updated. **Do not implement rehash mode in Phase 5** — document it as out-of-scope and replace the current "update row hashes" language with "verify row hashes (read-only)."

---

### [HIGH] Step 14 collision-dispatch query is not instance-scoped — cross-instance id collision inside the transaction produces a false `Deduplicated` result

**Category:** Logic Flaws

**Trigger:** Step 7 (pre-transaction check) was fixed in R9 to include `AND caddy_instance_id = self.instance_id`. However, the step-14 collision-dispatch branch that executes after catching a UNIQUE violation on the `id` PRIMARY KEY does not have the same scoping. The relevant query is:

`SELECT length(CAST(desired_state_json AS BLOB)) AS json_len FROM snapshots WHERE id = ? LIMIT 1`

There is no `AND caddy_instance_id = ?`. Consider: instance A (`caddy_instance_id = 'inst-a'`) has written a snapshot with hash `H`. Instance B (`caddy_instance_id = 'inst-b'`) writes a state whose canonical hash is also `H`. Step 7 queries `WHERE id=H AND caddy_instance_id='inst-b'` — no row found. Step 8 begins IMMEDIATE. Step 13 plain INSERT fails: PRIMARY KEY conflict (instance A's row holds `id=H`). Step 14 first check: `WHERE id=H AND config_version=new_version_for_B` — no row found (instance B has never written). Step 14 no-row branch: `SELECT length(...) WHERE id=H LIMIT 1` — finds instance A's row. `json_len == bytes.len()`, body matches — `tx.rollback().await?` and returns `WriteOutcome::Deduplicated`. Instance B's caller receives a success signal. Instance B has zero rows in `snapshots`.

**Consequence:** Instance B's caller believes the write succeeded (`Deduplicated` is a success outcome with a valid snapshot in the response). But `SELECT * FROM snapshots WHERE caddy_instance_id='inst-b'` returns zero rows. Every downstream consumer of instance B's history — Phase 7 rollback, Phase 9 API, `in_range`, `children_of` — sees instance B as having no state. Data loss with a success signal. This is the same failure class as the R9 finding for step 7, but in the intra-transaction collision path.

**Design assumption violated:** The design assumes that a `Deduplicated` result returned from the step-14 collision branch always means "a row for this instance exists with this content." After the R9 fix to step 7, the pre-transaction path is correct. Step 14's diagnostic query has no such scoping, so it conflates cross-instance PRIMARY KEY matches (different instances with identical content) with same-instance dedup (the same instance wrote this content before).

**Suggested mitigation:** Add `AND caddy_instance_id = self.instance_id` to both queries inside the step-14 collision-dispatch branch:
- `SELECT length(CAST(desired_state_json AS BLOB)) AS json_len FROM snapshots WHERE id = ? AND caddy_instance_id = ? LIMIT 1`
- The full-body fetch that follows if lengths match.

With instance scoping, finding no row in the id-present subquery means the PRIMARY KEY conflict came from a different instance — route to `VersionRace` (or introduce a dedicated `CrossInstanceCollision` variant for clarity). This matches the semantics of the pre-transaction step 7 and closes the cross-instance data-loss path inside the transaction.

---

## Summary

**Critical:** 1 &nbsp; **High:** 1 &nbsp; **Medium:** 0 &nbsp; **Low:** 0

**Top concern:** The `regen-snapshot-hashes` binary is architecturally blocked: migration 0004's RAISE ABORT triggers on UPDATE and DELETE make it impossible for the tool to modify any snapshot row, which is its stated purpose. The design specifies a "single SQLite transaction for all row updates" without addressing how those updates bypass triggers that unconditionally abort them. This is not a latent risk — it is a guaranteed failure the first time the binary is invoked after a canonical JSON version bump. Resolving this requires an ADR-ratified rehash strategy compatible with append-only immutability before implementation begins.

# Security Review — Phase 6 (Audit Log Subsystem)

Diff range: `5e0b13f..HEAD`

---

[HIGH] NO PRODUCTION CiphertextHasher IMPLEMENTATION EXISTS
File: core/crates/core/src/audit/redactor.rs, core/crates/adapters/src/audit_writer.rs
Lines: redactor.rs:24-28; audit_writer.rs:140-157
Description: `CiphertextHasher` is a public trait with three implementations — all three live exclusively in `#[cfg(test)]` blocks (`Sha256Hasher`, `PlaintextHasher`, `ZeroHasher`). There is no production struct implementing the trait anywhere outside tests. `AuditWriter::new` accepts a `SecretsRedactor<'static>`, which transitively requires a concrete `CiphertextHasher`. When the CLI or server wires this up (Phase 9), a developer could unknowingly supply a low-quality or constant hasher with no type-system enforcement preventing it. The `ZeroHasher` (all-zero output) used in adapter integration tests already demonstrates the problem: every redacted field emits `***000000000000` regardless of the actual secret, which is deterministically distinguishable and reveals nothing about the plaintext but allows an attacker to confirm whether two redacted values were the same original secret through correlation against external data (they will always be identical regardless of input).
Category: Cryptography / unsafe data handling
Attack vector: An operator reviewing the audit log sees `***000000000000` for all redacted fields. Correlation across rows is trivial; when combined with out-of-band knowledge about which secrets are in use, it confirms which secret was applied without revealing it directly.
Suggestion: Provide a concrete `Sha256AuditHasher` struct in the adapters crate (not in tests) that wraps `sha2::Sha256`. Gate `AuditWriter::new` to only accept this type, or assert at construction time that the hasher is not the zero-output constant. Document the security contract in the `CiphertextHasher` doc comment.

---

[HIGH] SILENT NULL FALLBACK ON REDACTED DIFF SERIALIZATION FAILURE BYPASSES AUDIT INTEGRITY
File: core/crates/adapters/src/audit_writer.rs
Lines: 173-177
Description: After the redactor runs successfully, the code serializes the redacted `serde_json::Value` back to a JSON string. The call is `serde_json::to_string(&redacted).unwrap_or_else(|_| "null".to_owned())`. A `serde_json::Value` serialization can only fail under extreme conditions (stack overflow on deeply recursive values, or custom `Serialize` impls — neither applies here), but the silent fallback replaces the entire redacted diff with the JSON literal `"null"` rather than propagating an error. This means a non-null diff is stored as `NULL` (or the string `"null"`) in the database with `redaction_sites > 0`, silently losing the audit record's operational content. The immutability triggers prevent a later correction.
Category: Error handling / information leakage
Attack vector: An attacker who can craft a pathologically deep or self-referential JSON diff could trigger this code path, causing the audit record's diff to be silently dropped while the row is still inserted with `outcome = ok` and `redaction_sites > 0`, creating a misleading audit trail.
Suggestion: Replace `unwrap_or_else` with `map_err` and propagate the error as `AuditWriteError::Redaction` (or a new `Serialization` variant). Do not swallow serialization failures silently when the consequence is a permanently incorrect audit record.

---

[WARNING] INCOMPLETE SECRET FIELD COVERAGE — TLS PRIVATE KEY PATH AND mTLS KEY NOT COVERED
File: core/crates/core/src/schema/secret_fields.rs
Lines: 11-16
Description: `TIER_1_SECRET_FIELDS` covers four paths: `password`, `forward_auth/secret`, `Authorization` header, and `api_key`. The codebase already contains `mtls_key_path` (referenced in `core/crates/core/src/config/types.rs:60,248,300,419`) and the broader concept of TLS private keys as configuration fields. If any future diff payload includes a PEM-encoded private key value (rather than a path), the field name would not match any registered pattern and the plaintext key would reach the audit log unredacted. Additionally, `token` is a common field name in HTTP auth configurations (e.g. bearer token values stored alongside upstream config) that is not covered.
Category: Secrets and sensitive data
Attack vector: A diff recording a configuration change that embeds a TLS private key, a bearer token value, or a JWT under a field not named exactly `password`, `secret`, `Authorization`, or `api_key` would pass through the redactor without redaction and be stored in plaintext in the audit log.
Suggestion: Audit all field names in the Caddy JSON schema that can carry secret material (private keys, bearer tokens, HMAC secrets, etc.) and add them to `TIER_1_SECRET_FIELDS`. Candidates include: `/tls/*/private_key`, `/upstreams/*/auth/token`, `/upstreams/*/auth/bearer`. Consider making this a living registry with a documented review gate when new upstream auth schemes are added.

---

[WARNING] HASH PREFIX ORACLE — 12-CHAR HEX TRUNCATION LEAKS 48 BITS OF SHA-256 OUTPUT
File: core/crates/core/src/audit/redactor.rs
Lines: 14-17, 161-167
Description: Redacted string fields are stored as `***<12-char-lowercase-hex>`, which is 48 bits of SHA-256 output. For short, low-entropy secrets (e.g., a 4-digit PIN, a short numeric API key, or a fixed-format token with a small keyspace), this prefix is sufficient to brute-force the original value by hashing all candidates and comparing the first 12 hex characters. SHA-256 is not a password hash — it has no work factor and no salt. An adversary with read access to the audit log can enumerate the candidate space offline.
Category: Cryptography / unsafe data handling
Attack vector: An attacker with read access to the `audit_log` table (e.g., via a compromised backup, a read-only DB user, or a misconfigured export) hashes all candidate short secrets and compares the first 48 bits against the stored `***` prefix to recover low-entropy secrets.
Suggestion: For password-class fields, use a keyed HMAC (HMAC-SHA256 with a server-side secret) or a salted slow hash (Argon2id/bcrypt) rather than a bare SHA-256 prefix. If stable correlation across rows is required (current use), use HMAC with a deployment-specific key stored outside the database. At minimum, document the low-entropy caveat prominently in the `CiphertextHasher` trait doc comment.

---

[WARNING] CORRELATION_ID ACCEPTED FROM UNTRUSTED X-Correlation-Id HEADER WITHOUT VALIDATION
File: core/crates/adapters/src/tracing_correlation.rs
Lines: 135-145
Description: `correlation_id_from_header` parses `X-Correlation-Id` from inbound HTTP requests and, if the value is a valid ULID, uses it directly as the correlation id for the entire request span and all audit rows emitted during that request. A client can therefore supply an arbitrary ULID that they previously observed in an audit log (e.g., by exploiting an information leak or by receiving a correlation id in a response header), causing their malicious request to be chained to a legitimate prior event in the audit log. This may impair forensic reconstruction of which requests were genuinely correlated.
Category: Authentication and authorisation / input validation
Attack vector: A malicious client sets `X-Correlation-Id: <ULID-of-a-prior-legitimate-request>`. All audit rows for the attacker's request are stored with the same `correlation_id` as the legitimate request, making it appear the attacker's actions were part of the original operation.
Suggestion: Accept `X-Correlation-Id` from callers only when those callers are trusted internal services (e.g., verified by mTLS or an internal bearer token). For external HTTP requests, always generate a fresh ULID server-side and optionally echo the client's value in a separate `X-External-Correlation-Id` field for cross-system tracing without poisoning the internal audit trail.

---

[WARNING] IMMUTABILITY TRIGGERS BYPASSABLE VIA SQLCIPHER ATTACH OR DIRECT FILE ACCESS
File: core/crates/adapters/migrations/0006_audit_immutable.sql
Lines: 31-42
Description: The `BEFORE UPDATE` and `BEFORE DELETE` triggers enforce immutability at the SQLite application layer. SQLite triggers are not a security boundary: anyone with write access to the `.db` file can use the SQLite CLI, `sqlite3_exec` with `PRAGMA writable_schema = ON`, or journal manipulation to modify rows without firing triggers. The WAL journal mode (`SqliteJournalMode::Wal`) configured in `sqlite_storage.rs:76` also means an attacker with filesystem access can replay or truncate the WAL to remove recent audit rows before a checkpoint. No hash-chain verification at read time is present in this diff to detect such tampering.
Category: Unsafe data handling / race conditions
Attack vector: A local attacker (or a compromised process running as the same OS user as the daemon) opens `trilithon.db` with the SQLite CLI, runs `PRAGMA writable_schema = ON; DELETE FROM audit_log WHERE actor_id = 'attacker';`, and commits. The triggers do not fire because `writable_schema` bypasses the trigger mechanism. The hash chain stored in `prev_hash` is never verified on read in this diff, so the deletion is not detected at query time.
Suggestion: The hash chain (`prev_hash`) computed in `record_audit_event` is the right defense here — add a `verify_audit_chain` function that re-walks the chain and confirms each row's `prev_hash` matches the SHA-256 of the prior row's canonical JSON. Expose this as a periodic health check or an operator CLI command. Document the filesystem-access caveat explicitly in the threat model.

---

[SUGGESTION] REDACT_DIFF DOES NOT GUARD AGAINST EXTREMELY DEEP JSON TREES (STACK OVERFLOW)
File: core/crates/core/src/audit/redactor.rs
Lines: 99-117, 123-153
Description: `SecretsRedactor::walk` and `self_check` are mutually recursive across arbitrarily deep `serde_json::Value` trees with no depth limit. A diff payload containing a deeply nested JSON object (e.g., 10,000 levels of nesting) would cause a stack overflow in the Tokio async runtime thread, potentially crashing the audit-writing task.
Category: Input validation
Attack vector: A caller that constructs or accepts an externally-sourced JSON document with deep nesting passes it as the `diff` field of `AuditAppend`. The redactor walks the tree recursively, exhausting the thread stack.
Suggestion: Add a `max_depth` parameter to `walk` and `self_check`, default it to a safe value (e.g., 64), and return `RedactorError::DepthExceeded` when exceeded. This prevents stack exhaustion and provides a clean error rather than a crash.

---

[SUGGESTION] NOTES AND TARGET_ID FIELDS HAVE NO LENGTH BOUNDS
File: core/crates/adapters/src/audit_writer.rs
Lines: 103-114
Description: `AuditAppend.notes` and `AuditAppend.target_id` are `Option<String>` with no length validation before storage. An internal caller passing unbounded strings (e.g., a full request body in `notes`) could insert very large rows into `audit_log`, growing the database without limit and potentially filling disk.
Category: Input validation
Attack vector: An internal component (not an external attacker, given there is no API surface yet) passes a multi-megabyte string in `notes`. The row is inserted, the immutability trigger prevents cleanup, and disk fills.
Suggestion: Add length caps (e.g., 4 KB for `notes`, 256 bytes for `target_id`) in `AuditWriter::record` before constructing the row, returning `AuditWriteError` on excess.

---

Security verdict: findings present — see above

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 -->

| # | Finding title | Status | Notes |
|---|--------------|--------|-------|
| 1 | [HIGH] NO PRODUCTION CiphertextHasher | SUPERSEDED (dde9dc5) | F009 - Sha256AuditHasher in adapters |
| 2 | [HIGH] SILENT NULL FALLBACK ON REDACTED DIFF | SUPERSEDED (dde9dc5) | F003 - Serialization variant |
| 3 | [WARNING] INCOMPLETE SECRET FIELD COVERAGE | Fixed | F023 - TLS private_key, bearer/token added |
| 4 | [WARNING] HASH PREFIX ORACLE - 48 BIT LEAK | Fixed | F013 - HMAC recommendation on CiphertextHasher |
| 5 | [WARNING] X-CORRELATION-ID UNTRUSTED HEADER | Fixed | F014 - trust boundary documented |
| 6 | [WARNING] IMMUTABILITY TRIGGERS BYPASSABLE | Fixed | F015 - verify_audit_chain added |
| 7 | [SUGGESTION] REDACT_DIFF NO DEPTH GUARD | Fixed | F029 - MAX_REDACTOR_DEPTH + DepthExceeded |
| 8 | [SUGGESTION] NOTES AND TARGET_ID NO LENGTH BOUNDS | Fixed | F030 - 4KiB/256B caps + FieldTooLong |

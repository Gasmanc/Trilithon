# Trilithon Native Bundle Format — Version 1

- **Format version:** 1.0
- **Date:** 2026-04-30
- **Status:** Stable

This page is the field-by-field specification for the Trilithon native
bundle format produced by Phase 25's exporter and consumed by Phase 26's
restore path. The implementation MUST track this document. Any
divergence between the implementation and this specification is a bug
in the implementation.

## 1. Compatibility promise

`schema_version: 1` is STABLE for V1.0:

- V1.x readers MUST read v1 bundles. There is no V1.x release that
  drops v1 bundle support.
- V2.x readers MUST read v1 bundles via a documented migration path
  published alongside the V2.0 release notes. Migration is a
  v1-to-v2 transcoding step; the v1 input bytes plus the migration
  code uniquely determine the v2 output.
- v1 bundles produced by V1.x MUST be byte-identical given identical
  inputs. The determinism test is the contractual evidence (§9).

## 2. Archive format

The bundle is a single `tar.gz` file:

- **Outer compression:** gzip, level 6, no filename in the gzip
  header, gzip mtime field set to `0`, gzip OS byte set to `0`
  ("FAT filesystem", the conventional value for "unspecified" in
  reproducible-build tooling).
- **Inner archive:** POSIX `ustar` tar. No PAX extended headers. No
  global headers. No sparse-file extensions.
- **Determinism rules** (every entry, every time):
  - `mtime = 0` (Unix epoch).
  - `uid = 0`, `gid = 0`.
  - `uname = ""`, `gname = ""`.
  - `mode = 0644` for files.
  - `mode = 0755` for directories.
  - Members are written in lexicographic order by member path.
  - No duplicated member paths.

The packer is implemented at
`core/crates/adapters/src/export/tar_packer.rs`. The determinism
rules are enforced at that single boundary.

## 3. Top-level layout

```
manifest.json
desired-state.json
snapshots/<snapshot_id>.json     (one file per snapshot, content-addressed)
audit-log.ndjson
secrets-vault.encrypted          (optional; absent if no secrets)
README.txt                       (human-readable orientation)
```

Member paths are exactly as listed. The `snapshots/` directory
contains one regular file per snapshot, named `<snapshot_id>.json`
where `<snapshot_id>` is the snapshot's content address (lowercase
hex SHA-256 of its canonical JSON serialisation, per ADR-0009 and
architecture §6.5).

## 4. `manifest.json`

UTF-8 JSON, object with deterministic key ordering (lexicographic at
every level). Every field below is documented with type, presence,
and an example.

| Field | Type | Presence | Example |
| --- | --- | --- | --- |
| `schema_version` | integer | required | `1` |
| `trilithon_version` | string (semver) | required | `"1.0.0"` |
| `caddy_version` | string (semver) | required | `"2.11.2"` |
| `exported_at` | string (RFC 3339 UTC) | required | `"2026-04-30T17:14:09Z"` |
| `root_snapshot_id` | string (content-addressed snapshot id, lowercase hex) | required | `"a3f1...e2c4"` |
| `snapshot_count` | integer | required | `42` |
| `audit_event_count` | integer | required | `1873` |
| `secrets_included` | boolean | required | `true` |
| `secrets_encryption` | object | present iff `secrets_included = true` | see below |
| `caddy_admin_endpoint_at_export` | string | required (human-reference only) | `"unix:///run/caddy/admin.sock"` |
| `bundle_sha256` | string (lowercase hex SHA-256) | required | `"7c01...90ab"` |

`secrets_encryption` shape:

```json
{
  "algorithm": "xchacha20poly1305",
  "kdf": "argon2id",
  "kdf_params": { "m_cost": 65536, "t_cost": 3, "p_cost": 4 }
}
```

`bundle_sha256` is the SHA-256 of all archive bytes EXCEPT the
`bundle_sha256` field itself. Computation: produce the manifest with
`bundle_sha256` set to a fixed 64-character placeholder string of
ASCII zeros, pack the archive, hash the resulting bytes, then
substitute the real digest in place of the placeholder, and re-pack.
Because every other input to the pack is deterministic, the
substitute-and-re-pack step is well-defined. Readers SHOULD verify
`bundle_sha256` against the bytes they received.

## 5. `desired-state.json`

The canonical Caddy JSON Trilithon would currently load if asked.
UTF-8 JSON, deterministic key ordering (lexicographic). This is the
single artefact a walk-away user can hand to stock Caddy via
`caddy run --config desired-state.json`.

## 6. `snapshots/<id>.json`

One file per snapshot. UTF-8 JSON. Schema:

```json
{
  "id": "<content-addressed snapshot id>",
  "parent_id": "<id or null for the root>",
  "actor": { "kind": "user|token|system", "id": "..." },
  "intent": "Operator's free-text description",
  "correlation_id": "<ULID>",
  "caddy_version": "2.11.2",
  "trilithon_version": "1.0.0",
  "created_at": 1745998849,
  "desired_state": { "...": "..." }
}
```

The filename `<id>.json` matches `id` byte-for-byte. The reader MAY
verify content-addressing by recomputing the SHA-256 of the
canonical JSON serialisation of the file body and comparing to the
filename stem. Mismatch is a corruption signal.

## 7. `audit-log.ndjson`

Newline-delimited JSON. One JSON object per line. Lines sorted by
`(created_at, id)` ascending. Each line:

```json
{
  "id": "<ULID>",
  "kind": "<domain.event>",
  "actor": { "kind": "user|token|system", "id": "..." },
  "correlation_id": "<ULID>",
  "target_type": "snapshot|mutation|route|...",
  "target_id": "...",
  "before_redacted": { "...": "..." },
  "after_redacted":  { "...": "..." },
  "created_at": 1745998849,
  "version": 1
}
```

`version` is the audit-log row schema version, initially `1`. The
`before_redacted` and `after_redacted` fields are diffs that have
already passed through the secrets-aware redactor (architecture §6.6,
hazard H10). Plaintext secrets do not appear in this file.

## 8. `secrets-vault.encrypted`

Present iff the manifest's `secrets_included` is `true`. Format: a
single XChaCha20-Poly1305 envelope laid out as raw binary bytes
(NOT base64):

```
[ nonce  : 24 bytes  ]
[ ciphertext : N bytes ]
[ tag    : 16 bytes  ]
```

The plaintext is a UTF-8 JSON object mapping secret-id to
metadata-and-ciphertext rows from `secrets_metadata` (architecture
§6.9). The master key is **NOT** in the bundle; a user restoring on a
new machine MUST provide the original master key out-of-band
(exported separately via the keychain export path).

KDF parameters and the algorithm identifier appear in the manifest's
`secrets_encryption` object. The salt for the KDF is stored alongside
the master key in the OS keychain on the original host; restoring on
a new machine requires both the bundle and the keychain export.

## 9. `README.txt`

A generated, human-readable orientation page. Plain ASCII (no
emoji). Required headings (each on its own line followed by a blank
line):

```
Trilithon native bundle (schema v1)

Exported at: <RFC 3339 timestamp>

What this is

If you no longer want to run Trilithon

Pointing stock Caddy at the desired state

Where the secrets live

Verifying the bundle
```

The "Pointing stock Caddy at the desired state" section MUST give
the literal command `caddy run --config desired-state.json`. The
"Verifying the bundle" section MUST describe the `bundle_sha256`
verification procedure.

## 10. Determinism test (contract evidence)

The named test that anchors the byte-determinism promise:

- **Test name:** `bundle::tests::deterministic_pack_is_byte_stable`.
- **File path:** `core/crates/adapters/src/export/bundle/tests.rs`.
- **Fixture:** `core/crates/adapters/tests/fixtures/bundle/sample.bundle.fixture.json`.
- **Procedure:** pack the fixture twice in two different temporary
  directories with two different system clocks (the packer ignores
  both), capture both archive byte streams, and assert they are
  byte-identical. A negative self-test deliberately re-introduces
  real `mtime` capture in the packer and asserts the test fails.

A passing run of this test is the operational definition of "v1
bundles produced by V1.x MUST be byte-identical given identical
inputs."

## 11. References

- ADR-0009 (immutable, content-addressed snapshots and audit log).
- Architecture §6.5 (snapshots), §6.6 (audit log), §6.9 (secrets).
- Phased plan, Phase 25 (configuration export), Phase 26 (backup
  and restore).

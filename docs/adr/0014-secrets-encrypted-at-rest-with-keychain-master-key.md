# ADR-0014: Encrypt secrets at rest with XChaCha20-Poly1305 and a keychain-managed master key

## Status

Accepted — 2026-04-30.

## Context

Trilithon's desired-state model contains fields that are secrets:
basic-auth passwords (T2.2 policy presets), API keys for forward-auth
(T3.4 sketch with V1 schema reservation), bot-challenge secrets,
backup encryption passphrases (T2.12), and the bootstrap account
credentials (T1.14, hazard H13). The binding prompt's T1.15 specifies
the contract: any field marked secret in the schema is stored
encrypted at rest, redacted from audit diffs, and never returned in
plaintext through any read endpoint except an explicit "reveal" call
that itself produces an audit entry. T1.15 acceptance: encryption
uses XChaCha20-Poly1305 with a key derived from a master key that
lives outside the SQLite database (system keychain on macOS / Linux,
file-with-restricted-permissions fallback). A leaked SQLite file
does not leak secrets.

Hazard H10 names the audit-log redaction issue. Hazard H13 names the
bootstrap-credential handling issue. Together with constraint 12
("Secrets never appear in audit log diffs in plaintext"), the
requirement is: the database-on-disk threat model treats the SQLite
file as semi-public; secrets are useless to an attacker who steals
only the file.

Forces:

1. **Threat model.** A laptop loss, a misconfigured backup, a
   compromised filesystem-level backup tool. Anywhere the SQLite
   file might land that is not the user's secured device. The
   master key MUST NOT be in the same place as the database.
2. **Algorithm choice.** XChaCha20-Poly1305 is named in the prompt.
   The 192-bit nonce removes the management burden of nonce reuse
   that plagues 96-bit nonce constructions. The construction is
   well-vetted (RFC 8439 ChaCha20-Poly1305, with the XChaCha
   extension defined by the libsodium project and standardised by
   the IRTF in draft form).
3. **Master-key location.** Operating-system keychains
   (`Security.framework` on macOS, the kernel keyring or
   Secret Service on Linux) provide hardware- or
   user-session-scoped key storage. A file fallback at `0600` is
   necessary for headless deployments and for systems without a
   keychain.
4. **Reveal is a privileged operation.** Reading a secret in
   plaintext is what the attacker would do. Trilithon must reveal
   secrets to legitimate operators (otherwise they cannot use them
   in upstream services) but must record every reveal in the audit
   log (T1.15 contract).

## Decision

**Algorithm.** Trilithon SHALL encrypt secret-marked fields using
XChaCha20-Poly1305 with a 192-bit (24-byte) nonce. Each ciphertext
SHALL be stored as a record containing: a version byte, the 24-byte
nonce, the ciphertext, and the 16-byte authentication tag.
Associated data SHALL include the field's logical identifier (the
secret's purpose key) so that copying a ciphertext between fields
fails authentication.

**Per-secret data key.** Each secret SHALL be encrypted with a
per-secret data key that is itself encrypted with a workspace key
(envelope encryption). The workspace key SHALL be stored in the
SQLite database, encrypted with the master key. The master key
SHALL never appear in the SQLite database in any form.

**Master key location.**

- **macOS.** The master key SHALL be stored in the user's keychain
  via `Security.framework`'s `SecItem` API, with an access control
  list scoped to the Trilithon daemon binary's signing identity
  (or to the binary path for unsigned development builds).
- **Linux with a Secret Service available** (GNOME Keyring,
  KWallet via Secret Service). The master key SHALL be stored
  through the Secret Service D-Bus API.
- **Linux headless or Secret Service unavailable.** The master
  key SHALL be stored in a file at the daemon's data directory,
  permission `0600`, owned by the daemon's process user. The file
  SHALL be excluded from automated backups by default; T2.12
  backups SHALL include the master key only when the user
  acknowledges the cross-machine restore requirement.
- **First-run.** If no master key exists at the configured
  location, Trilithon SHALL generate one (32 bytes from a CSPRNG)
  and store it. The generation SHALL be recorded in the audit log
  as a `master_key_initialised` event, with no ciphertext or
  plaintext fields.

**Key rotation.** The master key MAY be rotated through an
explicit `rotate_master_key` operation that re-encrypts the
workspace key under the new master key. Data keys SHALL NOT be
re-encrypted on master-key rotation (the envelope structure
makes it unnecessary). Per-secret key rotation MAY be performed
on demand for a single secret. Rotation events SHALL produce
audit rows.

**Audit redaction (hazard H10, constraint 12).** A secrets-aware
redactor SHALL sit between the diff engine and the audit log
writer (ADR-0009 decision). The redactor SHALL replace any field
marked secret in the schema with a stable placeholder (the SHA-256
of the plaintext, prefixed with `secret:`) so that audit consumers
can detect equality without learning the value. Plaintext secrets
SHALL NEVER reach the audit log writer; the type system (a
`RedactedDiff` newtype, ADR-0009) prevents the bypass.

**Reveal.** A secret's plaintext SHALL be returned only through an
explicit `reveal_secret` operation. `reveal_secret` SHALL require
a user-authenticated session (T1.14). `reveal_secret` SHALL write
an audit row recording the actor, the secret identifier, and the
correlation identifier. `reveal_secret` SHALL NOT be exposed
through the language-model tool gateway (ADR-0008); language
models SHALL receive the redacted placeholder, never the
plaintext.

**Bootstrap credentials (hazard H13).** The bootstrap account
credentials SHALL be written to a permission-restricted file
(`0600`) in the daemon's data directory on first run. The
credentials SHALL NOT appear in process arguments, environment
variables, or logs. The user SHALL be prompted to change them
on first login. Once changed, the bootstrap file SHALL be
deleted by the daemon and the deletion SHALL be recorded in the
audit log.

**Failure handling.** If the master key cannot be loaded at
startup, the daemon SHALL refuse to start and SHALL emit a
structured error explaining the failure path and the recovery
options. The daemon SHALL NOT silently regenerate the master key,
because regeneration would render existing ciphertexts
unrecoverable.

## Consequences

**Positive.**

- A leaked SQLite file does not leak secrets. The threat model
  T1.15 names is honoured by construction.
- The audit log redactor is non-bypassable through the type
  system, addressing hazard H10 structurally.
- Reveal is auditable. Every plaintext access produces a row.
- Master-key rotation is cheap thanks to envelope encryption;
  data keys do not need re-encryption.

**Negative.**

- The keychain integration adds platform-specific code in
  `crates/adapters` (macOS `Security.framework` bindings, Linux
  Secret Service D-Bus client, file fallback). The matrix is a
  real maintenance burden.
- A user who loses the master key loses access to their secrets.
  T2.12 backups can include the master key, but doing so widens
  the blast radius of a backup compromise. The trade is the
  user's; documentation SHALL explain it.
- Cross-machine restore (T2.12 acceptance: "Restore on a different
  machine produces an identical desired state") requires
  transporting the master key to the new machine, with all the
  attendant secret-handling concerns. The restore flow SHALL guide
  the user through this without adding a "send the key over the
  network" path.

**Neutral.**

- The XChaCha20-Poly1305 implementation MAY come from `chacha20poly1305`
  (RustCrypto) or libsodium. The choice is a `crates/adapters`
  concern recorded in the architecture document; both implementations
  satisfy this ADR's contract.
- Hardware-backed keys (Secure Enclave, TPM) are a Tier 3 sketch.
  V1 uses software-managed keys exclusively.

## Alternatives considered

**AES-256-GCM with a 96-bit nonce.** The default mainstream
authenticated encryption mode. Rejected because the 96-bit nonce
imposes a strict nonce-management discipline that XChaCha20-Poly1305
removes; the prompt names XChaCha20-Poly1305 explicitly.

**Plaintext at rest with filesystem-level encryption only.** Rely
on the user's disk encryption. Rejected because filesystem-level
encryption protects against device theft when off, not against
backup compromise, not against a process running as another user
with read access, and not against the SQLite file traveling through
copy-paste during support interactions. The threat model demands
application-level encryption.

**Master key derived from a user passphrase on every boot.**
Argon2id-derive the key from a user-entered passphrase at startup.
Rejected because headless deployments (T2.7, T2.8) cannot prompt
interactively, because passphrase entropy is variable, and because
the keychain fallback already provides the equivalent property
(the master key sits behind the OS-managed user session).

**External secret manager (HashiCorp Vault, AWS Secrets Manager).**
Delegate secret storage to a managed service. Rejected because
constraint 14 makes the user sovereign over their data and because
Trilithon's local-first deployment target does not assume a managed
service is available.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#2-non-negotiable-constraints`,
  items 12, 14; section 4 features T1.7, T1.14, T1.15; section 5
  feature T2.12; section 7 hazards H10, H13, H14.
- ADR-0006 (SQLite as V1 persistence layer).
- ADR-0008 (Bounded typed tool gateway for language models).
- ADR-0009 (Immutable content-addressed snapshots and audit log).
- ADR-0011 (Loopback-only by default with explicit opt-in for remote
  access).
- RFC 8439 (ChaCha20 and Poly1305 for IETF Protocols).

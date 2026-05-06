# Phase 4 — Security Review Findings

**Reviewer:** security
**Date:** 2026-05-06
**Diff range:** 43f89ca..a948fa8
**Phase:** 4

---

[WARNING] OPEN REDIRECT — NO URL VALIDATION ON REDIRECT TARGET
File: core/crates/core/src/model/redirect.rs
Lines: 8-12
Description: `RedirectRule.to` is a free-form `String` with no validation at the model layer. There is also no validation in `validate.rs` — the `ImportFromCaddyfile`, `CreateRoute`, and `UpdateRoute` pre-condition checks do not inspect the `redirects` field at all. An attacker who can supply a `CreateRoute` or `UpdateRoute` mutation can set `to` to an arbitrary URL including `javascript:`, `data:`, `//evil.com/`, or a protocol-relative URL that turns the proxy into an open redirector pointing off-domain.
Category: Input validation
Attack vector: Submit a `CreateRoute` mutation with `redirects.to = "//attacker.example"` (or any URL the operator did not intend). The value passes all existing pre-condition checks unchanged and is stored into `DesiredState` unmodified.
Suggestion: In `validate.rs`, add a check for `CreateRoute` and `UpdateRoute` (and `ImportFromCaddyfile` per synthesised route) that validates `redirects.to` — at minimum reject empty strings and anything without an `http://` or `https://` scheme prefix, or restrict to same-origin path redirects. A simple `url::Url::parse` + scheme allowlist (`["http", "https"]`) is sufficient.

[WARNING] UNVALIDATED CIDR MATCHER — NO SYNTAX CHECK
File: core/crates/core/src/model/matcher.rs
Lines: 64-66
Description: `CidrMatcher(pub String)` is a free-form string. The `pre_conditions` validator in `validate.rs` does not inspect `MatcherSet.remote` entries at all. An invalid CIDR (e.g. `"not-a-cidr"`, a gigantic prefix, or a prefix with extra octets) will be accepted into `DesiredState` and only fail later when Caddy tries to apply it, producing an opaque apply error rather than a clear rejection at mutation time.
Category: Input validation
Attack vector: Submit a `CreateRoute` or `UpdateRoute` mutation with a `matchers.remote` entry containing a malformed CIDR. The state is committed and subsequent apply attempts fail silently or return Caddy-level errors that are harder to attribute.
Suggestion: Add a CIDR format check in a `check_matchers_valid` function called from the `CreateRoute` and `UpdateRoute` pre-condition paths. Use `std::net::Ipv4Addr`/`Ipv6Addr` + prefix-length parsing or the `ipnet` crate to validate each `CidrMatcher` string.

[WARNING] IDENTIFIER NEWTYPES ACCEPT ARBITRARY STRINGS — NO FORMAT ENFORCEMENT
File: core/crates/core/src/model/identifiers.rs
Lines: 8-54
Description: `RouteId`, `UpstreamId`, `PolicyId`, `PresetId`, and `MutationId` are `pub(String)` newtypes. The `new()` constructor produces a valid ULID, but deserialization via serde accepts any `String`. An external caller can therefore supply a mutation with `id = "../../etc/passwd"`, `id = ""`, an extremely long string, or a string containing characters that have meaning in JSON Pointer or file-path contexts. These raw id strings flow into hint messages, log lines, and potentially into storage keys (Phase 5+) where no prior sanitisation will have occurred.
Category: Input validation
Attack vector: Submit a mutation carrying an `id` that is empty, exceeds a reasonable length, or contains control characters. The value passes all existing checks and is persisted into `DesiredState.routes`/`upstreams` as a BTreeMap key, propagating into audit hint strings and future storage operations.
Suggestion: Add a `from_external` constructor (or a serde `Deserialize` impl) on the id newtypes that validates the inner string against a ULID character-set regex (`[0-9A-Z]{26}`) and a fixed 26-character length. Accept only that format from untrusted input.

[WARNING] DIFF SERIALISATION SILENTLY DISCARDS FAILURES
File: core/crates/core/src/mutation/apply.rs
Lines: 423-430
Description: The `to_json` helper calls `serde_json::to_value(v).ok()`, swallowing any serialization error and returning `None`. This means a `DiffChange.before` or `DiffChange.after` field will silently be `None` if serialization fails. In a security context, an incomplete diff could cause the audit record to omit the "before" value of a changed field, reducing the forensic value of the audit log and making it impossible to detect what was actually changed in a contested mutation.
Category: Error handling and information leakage
Attack vector: Any internal serialization failure silently produces an incomplete audit diff with a `null` before-value, obscuring what state was modified.
Suggestion: Either propagate the serialization error from `to_json`, or log a structured warning via `tracing::warn!` before returning `None`, so silent omissions are at least observable.

[WARNING] `on_demand_ask_url` ACCEPTED WITHOUT FORMAT VALIDATION
File: core/crates/core/src/model/tls.rs
Lines: 15-17
Description: `TlsConfig.on_demand_ask_url` (and its patch counterpart) is a free-form `Option<String>`. Caddy's on-demand TLS feature uses this URL to decide at certificate-issuance time whether to obtain a cert for a hostname. There is no validation in `validate.rs` for `SetTlsConfig`. An operator or compromised client could set this to an internal service URL (SSRF), a non-HTTPS URL, or a URL pointing to a controlled server that always returns 200 for every hostname — effectively disabling the on-demand hostname allowlist and enabling certificate issuance for arbitrary domains.
Category: Input validation
Attack vector: Submit a `SetTlsConfig` mutation with `on_demand_ask_url = "http://attacker.internal/always-yes"`. Once applied, Caddy will query this URL before issuing on-demand certificates, and an always-200 response allows cert issuance for any hostname the attacker routes to the proxy.
Suggestion: In `validate.rs`, add a pre-condition for `SetTlsConfig`: if `patch.on_demand_ask_url` is `Some(Some(_))`, parse the URL and reject any non-`https` scheme and any loopback/RFC 1918 destination (SSRF guard).

[SUGGESTION] `schemars` DEPENDENCY NOT VERSION-PINNED
File: core/crates/core/Cargo.toml
Lines: general
Description: `schemars = "0.8"` uses a bare major-version constraint. A supply-chain compromise or unexpected API change in a `0.8.x` release would be picked up silently on the next `cargo update`. The `#[derive(JsonSchema)]` macro runs at compile time across all model types, making it a broader attack surface than a runtime dependency.
Category: Dependency and configuration
Suggestion: Pin to an exact version (`schemars = "=0.8.21"` or current) or move it to the workspace `[dependencies]` table so `cargo deny check` and the workspace lock govern it uniformly.

[SUGGESTION] `proptest` DEPENDENCY NOT VERSION-PINNED
File: core/crates/core/Cargo.toml
Lines: general
Description: `proptest = "1"` in `[dev-dependencies]` uses only a major-version constraint. A compromised `proptest 1.x.y` release would execute at test time (including CI), which could exfiltrate secrets present in the CI environment (repository tokens, signing keys, deployment credentials).
Category: Dependency and configuration
Suggestion: Pin to a specific version (`proptest = "=1.6.0"` or current) in the workspace dev-dependencies table.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-06 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Open redirect — no URL validation on redirect target | ✅ Fixed | `21e330d` | — | 2026-05-06 | F049 — check_redirect_url validates http/https scheme |
| 2 | Unvalidated CIDR matcher — no syntax check | ✅ Fixed | `21e330d` | — | 2026-05-06 | F051 — check_matchers_valid with parse_cidr() |
| 3 | Identifier newtypes accept arbitrary strings | ✅ Fixed | `d826850` | — | 2026-05-06 | F006 — ULID validation on deserialization |
| 4 | Diff serialisation silently discards failures | ✅ Fixed | `21e330d` | — | 2026-05-06 | F029 — to_json now logs tracing::warn! on failure |
| 5 | on_demand_ask_url accepted without format validation | ✅ Fixed | `21e330d` | — | 2026-05-06 | F050 — check_on_demand_ask_url: https + SSRF guard |
| 6 | schemars dependency not version-pinned | ✅ Fixed | `3971998` | — | 2026-05-06 | F056 — pinned to =0.8.22 |
| 7 | proptest dependency not version-pinned | ✅ Fixed | `3971998` | — | 2026-05-06 | F056 — pinned to =1.11.0 |

# Security Design — providers (U4)

Consumes: `nfr-requirements/security-requirements.md` (NFR1.1–1.14,
NFR2.1), `functional-design/rules.md`, `functional-design/entities.md`,
`inception/contract-design/contract-summary.md` (C1, C4).
Questions: `nfr-design-questions.md` Q1–Q3, Q7 (conductor-decided).

## Defense-in-depth layout

| boundary | control | covers |
|----------|---------|--------|
| Transport (RPC) | `admin.oxpilot.*` SessionOnly by omission from PAT classifications + `require_admin` — bearer/PAT refused pre-dispatch (`auth.pat_scope`), anonymous `auth.unauthenticated`, non-admin `admin.forbidden`/`auth.banned`; closed PROCEDURES-set test pins registration | NFR1.1, NFR1.3 |
| Feature gate | `admin.oxpilot.*` and `oxpilot.status` are gate-EXEMPT — the admin surface is the recovery path; everything else keeps failing `oxpilot.disabled` | NFR1.2 |
| Egress | `validate_base_url` SSRF bound + `redirect(Policy::none())` client builder — no validated-URL redirect onto private hosts | NFR1.8, NFR1.9 |
| Secrets | write-only custody: encrypt-once, decrypt-in-construction-only, resolved-source `last4` reads | NFR1.4–1.6 |

## Custody helpers (Q1)

```rust
// crates/oxidean-api-oxpilot/src/providers/key.rs — the module lives
// INSIDE the providers tree so the visibility bound actually enforces
const KEY_ENVS: &[&str] = &["OXIDEAN_OXPILOT_SECRETS_KEY", "OXIDEAN_SESSION_SECRET"];

pub(crate) fn encrypt_key(plain: &str) -> Result<(String, String)>; // (ciphertext_hex, last4)
pub(in crate::providers) fn decrypt_key(ct: &str) -> Result<String>; // construction only
pub(crate) fn key_last4(plain: &str) -> String;                   // env-key read path
```

`decrypt_key` is `pub(in crate::providers)` and called only from
`providers::build` — `settings.rs`/`egress.rs` are same-crate, so
`pub(crate)` would NOT enforce the boundary; the `pub(in …)` scope makes
a read-path decrypt a compile failure, not a remembered rule (NFR1.4).
`clear_key` / `clear_overrides` NULL ciphertext+last4 together (NFR1.5).

## SSRF validator (Q2)

```rust
// crates/oxidean-api-oxpilot/src/providers/egress.rs
pub async fn validate_base_url(provider_id: Option<&str>, url: &str) -> Result<()>;
// 1. Url::parse + scheme ∈ {http, https} else invalid_provider
// 1b. if provider_id.is_none() → done (no consumer exists — provider-
//     less configs store URLs shape-checked only, BR3.2)
// 2. if provider_id == Some("ollama") → done (self-host exception, NFR1.10)
// 3. host: literal IP forms first (dotted via IpAddr::from_str; hex 0x,
//    octal 0, integer, IPv4-mapped-IPv6 hand-parsed) → reject private/
//    loopback/link-local/unspecified/multicast/169.254.169.254
// 4. DNS: tokio::net::lookup_host — EVERY resolved address must pass
//    the same public check; zero answers ⇒ invalid_provider
// TOCTOU bound: DNS can re-resolve differently at connect — documented
// operator caveat; literal public IP or the Ollama adapter is the
// recommended shape for private-network deployments
```

Provider clients are built with
`reqwest::Client::builder().redirect(redirect::Policy::none())` — a 3xx
surfaces as `ProviderError{code}` (NFR1.9).

## Registry + proc placement (Q3)

```rust
// crates/oxidean-api-oxpilot/src/providers/registry.rs
pub static REGISTRY: &[ProviderDescriptor] = &[OPENAI, ANTHROPIC, OLLAMA];
// RESOLUTION_MAP = REGISTRY + StubProvider — the closed set `build()`
// resolves against (4 members, BR1.4/NFR3.8); REGISTRY alone (3) is the
// providers_list surface (stub omitted, NFR1.14)
pub async fn build(cfg: &EffectiveProviderConfig) -> Result<Arc<dyn LlmProvider>, ProviderError>;
```

Stub reachability is split by path: `build()` accepts `stub` whenever it
is resolved (env `OXIDEAN_OXPILOT_PROVIDER=stub` stays legal in every
build — U1 `build_oxpilot_slot` parity, CONFIGURATION.md) but
`settings_set` refuses `provider_id="stub"` with `invalid_provider`
unless `cfg!(debug_assertions)` — dev/test builds can select it, release
builds cannot (BR1.4). `providers_list` reads REGISTRY plus one
`oxpilot_settings` row read — `configured` ("selectable now") requires
key material resolvable for key-requiring providers, so the stored-key
presence check is part of the read, not just the static table (NFR1.14).

## Audit + secret-surface design (Q7)

`audit_oxpilot_settings(ctx, action, &[names])` writes
`{actor, action, field_names, ts}` inside the mutex section after the
swap — names only; a key-field audit records `"api_key"` never a value
(NFR5.1, NFR1.7). Errors use the shipped codes exactly:
`auth.unauthenticated` / `auth.pat_scope` / `admin.forbidden` /
`auth.banned` / `rpc.bad_input` / `invalid_provider` + the one additive
`crypto.unavailable` (NFR1.13).

## Threat-residual statements

- Ollama's private-target exemption is the intended hole (NFR1.10) —
  disclosed in CONFIGURATION.md (NFR2.1).
- DNS TOCTOU is a documented bound, not a hole — validation happens
  pre-commit; an operator needing airtight egress isolation runs the
  feature behind their own proxy or uses a literal IP (NFR1.8).

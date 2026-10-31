# Security Requirements — providers (U4)

Consumes: `construction/providers/functional-design/rules.md`
(BR1.1–BR5.6), `functional-design/entities.md`,
`inception/requirements-analysis/requirements.md` (NFR1, NFR2),
`inception/contract-design/contract-summary.md` (C1, C4),
`codekb/oxidean-app/technology-stack.md`.

## Authentication & authorization

| id | requirement | basis |
|----|-------------|-------|
| NFR1.1 | `admin.oxpilot.*` requires a session-authenticated admin actor (`require_admin`); bearer/PAT credentials are denied by the SessionOnly catch-all — the proc set is pinned closed in `oxidean-api-oxpilot`'s PROCEDURES + dispatch by a test asserting both directions | NFR1, BR1.1, BR5.4 |
| NFR1.2 | `admin.oxpilot.*` is enablement-gate-EXEMPT (like `oxpilot.status`) — the admin surface is the recovery path for a disabled/misconfigured feature and must answer while `oxpilot.disabled` is everywhere else's verdict | BR1.1, R-20 |
| NFR1.3 | Non-admin session actors get `admin.forbidden`/`auth.banned`; missing session `auth.unauthenticated`; bearer/PAT `auth.pat_scope` — the shipped code names, not C1's sketch literals | NFR1, BR5.1 |

## Secret custody & data protection

| id | requirement | basis |
|----|-------------|-------|
| NFR1.4 | Provider key material is write-only: `api_key` plaintext is accepted once by `settings_set`, encrypted AES-256-GCM via `crate::crypto` under `["OXIDEAN_OXPILOT_SECRETS_KEY","OXIDEAN_SESSION_SECRET"]`, stored as ciphertext+last4; decrypt happens ONLY inside provider construction memory — never on a read, never logged, never emitted | NFR1, BR1.2, Q2 |
| NFR1.5 | `key_ciphertext` and `key_last4` are always written/cleared together — no half-written key row is observable | entities, BR3.4 |
| NFR1.6 | `settings_get` returns `key_set`+`key_last4` computed against the RESOLVED key source (stored last4 or env-key last4) — no ciphertext, no plaintext, ever; `providers_list` carries NO key fields at all (its contract shape is `{id, models, configured}`) | NFR1, BR2.4, C1 |
| NFR1.7 | No agent-reachable surface (RPC response, SSE frame, audit row, log line, tracing attr) emits secret material; audit carries field NAMES only — values never audited, not even hashed | NFR1/NFR2, BR5.2, Q6 |

## Egress control (SSRF bound)

| id | requirement | basis |
|----|-------------|-------|
| NFR1.8 | `base_url` must be http(s); for non-`ollama` resolved providers the host is forbidden when it is private/loopback/link-local/metadata — classified across literal IP notations (dotted, hex, octal, integer, IPv6, IPv4-mapped-IPv6) AND DNS-resolved answers (every answer public). The DNS check is validation-time with a documented TOCTOU bound | NFR1, BR3.2, Q3 |
| NFR1.9 | Provider HTTP clients set `redirect(Policy::none())` — a validated URL cannot 302 onto private/metadata egress; redirects surface as `ProviderError`, never auto-followed | NFR1, BR4.5 |
| NFR1.10 | The Ollama adapter is the documented self-host exception — the ONLY adapter permitted private/loopback targets (keyless, `http://127.0.0.1:11434` default) | US5.4/AC5.4.1, BR4.4, BR3.2 |
| NFR1.11 | Provider-less resolved configs skip the private-target bound (no consumer) but keep the http(s) shape check | BR3.2 |

## Integrity & audit

| id | requirement | basis |
|----|-------------|-------|
| NFR1.12 | Every settings mutation writes an audit row {actor, action, field_names_only, ts} inside the mutex-serialized section — audit order matches commit order. A crash between persist and audit can lose the row (persist+audit are not one transaction — a bounded, documented gap, not a claim of crash-safety) | BR5.2, BR4.3, R-14 |
| NFR1.13 | `crypto.unavailable` (missing key source) is the ONE additive error code — pinned so codegen cannot invent a substitute | BR5.1, R-17 |
| NFR1.14 | `stub` is registry-internal only: refused by `settings_set` (`invalid_provider`) and omitted from `providers_list` outside test builds — a test adapter can never be selected as a production provider | BR1.4 |

## Threat considerations

STRIDE-lite for providers: **Spoofing** — admin-session-only write
surface + SessionOnly catch-all (NFR1.1–1.3). **Tampering** —
mutex-serialized write section; ciphertext+last4 atomicity
(NFR1.5, NFR1.12). **Repudiation** — field-names-only audit on every
mutation (NFR1.12). **Information disclosure** — write-only key custody,
resolved-source last4 only, value-free audit, no-secret surfaces
(NFR1.4–1.7). **Denial of service** — bounded timeouts and
connect-phase-only retry live in reliability-requirements (NFR4.4–4.5);
admin-write serialization prevents divergent swaps. **Elevation of
privilege** — the Ollama private-target exemption is documented as the
single intended hole (NFR1.10); gate-exempt admin procs are the
documented recovery surface (NFR1.2).

## Privacy disclosure

| id | requirement | basis |
|----|-------------|-------|
| NFR2.1 | `docs/CONFIGURATION.md`'s oxpilot section discloses key custody (write-only, AES-256-GCM chain, rotation via `clear_key`+re-set) AND the egress posture (Ollama is the sole private-target adapter; DNS-validated bound carries a TOCTOU caveat) — the disclosure ships with all three legs of the env-doc pairing: `.env.example` + `docs/CONFIGURATION.md` + `.railway/railway.ts` (BR5.6) | NFR2, BR5.6 |

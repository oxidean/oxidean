# Security Design — u1-skeleton-slice

Consumes: `nfr-requirements/security-requirements.md` (NFR1.1–1.13,
NFR2.1), `functional-design/rules.md`, `functional-design/entities.md`,
`inception/contract-design/contract-summary.md` (C1, C2).
Questions: `nfr-design-questions.md` Q4 (conductor-decided).

## Defense-in-depth layout

The skeleton has three trust boundaries, each with an explicit control:

| boundary | control | covers |
|----------|---------|--------|
| Transport (HTTP/RPC/SSE) | Session-cookie auth on every surface; `oxpilot.*` procedures are SessionOnly **by omission** — never listed in `proc_classes` PAT tiers, so PAT/`ox`/MCP bearers are refused pre-dispatch; a closed-set test asserts PROCEDURES ↔ dispatch arms ↔ classification in both directions | NFR1.1, NFR1.2, NFR1.3 |
| Feature gate | `EnablementGuard` — a shared helper resolving `OXIDEAN_OXPILOT_ENABLED` (DB `oxpilot_settings.enabled` override) + provider slot; precedence `flag_off > misconfigured > no_provider`; every surface except `oxpilot.status` fails closed with `oxpilot.disabled` | NFR1.5 |
| Data (session rows) | Ownership check on every session-scoped query — `WHERE user_id = caller`; cross-user/absent both return the shared `not_found` (anti-enumeration, no oracle) | NFR1.4 |

## Check ordering design (Q4)

Every oxpilot surface executes **auth → gate → ownership** in that order,
implemented per-surface via `EnablementGuard`, not stacked middleware:

```text
// interface-level shape, ≤15 lines per stage constraint — two levels:
fn resolve_enablement(ctx) -> EnablementState            // pure resolver, never denies
fn require_enabled(ctx) -> Result<EnablementState, AppError>  // maps !enabled -> oxpilot.disabled
// gated RPC procs: session ctx (pre-auth'd) → require_enabled()? → proc body
// SSE handlers: session cookie → require_enabled()? → owned_session(id)? → stream
// oxpilot.status: gate-exempt — resolve_enablement() → REPORT {enabled, provider, reason}
```

`resolve_enablement` returns the `EnablementState` value itself, so the
gate-exempt `status` proc can report `{enabled:false, reason}`; the
`require_enabled` guard step wraps it and `?`-propagates
`oxpilot.disabled{reason}` on gated surfaces. A middleware layer was
rejected: `oxpilot.status` must be gate-exempt, and Axum layering for
one exemption reads worse than an explicit helper call each handler
makes. `EnablementGuard` keeps the ordering identical across RPC and
SSE — parity is structural, not remembered (NFR1.3).

## Fail-closed deny design

| deny | surface behaviour | covers |
|------|-------------------|--------|
| flag_off | `oxpilot.disabled{reason:"flag_off"}` on gated procs + both SSE routes; `oxpilot.status` still answers `{enabled:false, reason}` | NFR1.5, NFR1.6 |
| misconfigured / no_provider | same typed error, distinct `reason` values | NFR1.5 |
| cross-user / absent | shared `not_found` — identical body, identical timing class | NFR1.4 |

`oxpilot.status` returns **only** `{enabled, provider{id,model}|null,
reason}` — never env values, file paths, or credential-shaped material
(NFR1.6).

## Secret custody design

- U1 holds **zero** key material: `OXIDEAN_OXPILOT_PROVIDER`/`_MODEL` are
  config strings; the custody chain (AES-256-GCM env-key encryption,
  verifier-class hashing) is documented as applying to U4's key slot and
  U6's verifier — no plaintext-credential columns exist in U1's tables
  (NFR1.7).
- `redact()` is a **new U1-authored detector** in `oxidean-core`: a
  pattern-table over known secret shapes (bearer/JWT/PEM/key idioms)
  applied on message write. The runner's `mask_secrets` is known-value
  masking and cannot serve this — correction recorded in NFR1.8 (NFR1.8).
- No agent-reachable surface emits secret material: RPC responses carry
  ids/state, SSE frames carry redacted deltas, audit `detail` carries
  `{session_id, repo_owner, repo_name, branch}` only, log lines are
  metadata-only (NFR1.9, NFR5.2).

## Integrity design

| attack | control | covers |
|--------|---------|--------|
| Double-turn race | Insert-if-absent claim on the per-session ActiveTurn map — the claim IS the decision, never check-then-act; loser gets `409 turn_active` | NFR1.10 |
| Client retry duplicate | `client_msg_id` per-session uniqueness; resend-after-terminal returns persisted state without a new row or turn | NFR1.11 |
| Oversize body | `content` ≤ 64 KiB at body validation ⇒ `422 invalid_body` inside C2's closed vocabulary | NFR1.12 |
| Budget abuse | **deferred to U2** (FR6.7) — recorded accepted gap, not silent | NFR1.13 |

## Privacy disclosure design

`docs/CONFIGURATION.md`'s oxpilot section discloses plaintext transcript
storage + redact-on-write + per-user isolation in the same change that
adds the env vars — the disclosure ships with the surface (NFR2.1).
`.env.example` carries matching comments.

## STRIDE-lite mapping

Spoofing → session-cookie-only auth (NFR1.1–1.3). Tampering →
anti-enumeration ownership filter (NFR1.4). Repudiation →
create-only audit in U1; delete audit is U2's proc (NFR5.1).
Information disclosure → fail-closed gate + trimmed status + redact +
metadata-only surfaces (NFR1.5–1.9). DoS → one-turn-per-session +
64 KiB body cap (NFR1.10, NFR1.12); budgets deferred (NFR1.13).
Elevation → no role surface in U1; SessionOnly omission blocks the
PAT/MCP path entirely.

# Security Requirements — u1-skeleton-slice

Consumes: `construction/skeleton-slice/functional-design/rules.md`
(BR1.1–BR1.5, BR2.x, BR3.x, BR4.1–BR4.2, BR5.2),
`functional-design/entities.md`,
`inception/requirements-analysis/requirements.md` (NFR1, NFR2, custody
constraint block),
`inception/contract-design/contract-summary.md` (C1, C2),
`codekb/oxidean-app/technology-stack.md`.

## Authentication & authorization

| id | requirement | basis |
|----|-------------|-------|
| NFR1.1 | Every `oxpilot.*` procedure resolves `SessionOnly` — achieved by absence from `proc_classes` PAT tiers, proven by a closed-set test asserting both directions (PROCEDURES↔arms↔classification); PAT/`ox`/MCP bearers are refused pre-dispatch | NFR1, BR1.2 |
| NFR1.2 | Web routes `/oxpilot` + `/oxpilot/$session` require an authenticated session; anonymous visitors get the standard auth redirect, never the page | NFR1, BR5.2, AC1.1.5 |
| NFR1.3 | Both SSE routes apply the identical auth → gate → ownership check order as RPC handlers — transport parity, no HTTP bypass | NFR1, BR4.1, C2 |
| NFR1.4 | Cross-user access to any session-scoped resource returns `not_found`, indistinguishable from an absent row (anti-enumeration); the denial MAY emit a fire-and-forget security audit — optional per BR3.3's violation note | NFR1/NFR2, BR3.3 |

## Gate & deny surface

| id | requirement | basis |
|----|-------------|-------|
| NFR1.5 | Disabled feature fails closed: every `oxpilot.*` proc except `oxpilot.status` and both SSE routes deny `oxpilot.disabled` server-side; reason precedence `flag_off > misconfigured > no_provider` | NFR1, BR2.1–BR2.3, BR2.5 |
| NFR1.6 | `oxpilot.status` is the sole gate-exempt surface and returns only `{enabled, provider{id,model}\|null, reason}` — never env values, paths, or key material | NFR2, BR2.3 |

## Secret custody & data protection

| id | requirement | basis |
|----|-------------|-------|
| NFR1.7 | U1 holds no provider key material — `OXIDEAN_OXPILOT_PROVIDER`/`_MODEL` are config strings, not secrets; the custody chain (AES-256-GCM env-key, verifier-class hashing) is documented as applying to U4's key slot and U6's verifier material, with zero plaintext-credential columns in the U1 tables | custody constraint, BR2.5, entities |
| NFR1.8 | Message content is secrets-redacted on write — U1 **authors** the `redact()` detector (shared util in `oxidean-core`, pattern-table over known secret shapes: bearer/JWT/PEM/key idioms). Correction register: functional-design BR3.5/entities say "existing secrets-pattern filter" — none exists in-repo (only the runner's `mask_secrets` known-value masker, which cannot serve this); U1 owns the new util and U2+ reuse it | NFR1, BR3.5, R-03 finding |
| NFR1.9 | No agent-reachable surface (RPC response, SSE frame, audit `detail`, log line) ever emits secret material; audit `detail` carries ids/owner/repo only, never message content | NFR1/NFR2, Q3/Q4 |

## Abuse & integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR1.10 | Concurrent-turn integrity is atomic: the 409 `turn_active` decision is an insert-if-absent claim on the per-session ActiveTurn map, never check-then-act | BR4.2, BR4.6 |
| NFR1.11 | `client_msg_id` idempotency prevents duplicate transcript writes on client retry; resend-after-terminal answers with persisted state, never a second row or new turn | BR3.6 |
| NFR1.12 | Chat POST body is size-capped (`content` ≤ 64 KiB ⇒ 422 `invalid_body` — oversize rides the body-validation failure so C2's closed error vocabulary stays untouched) — closes the one-turn bound's unbounded-body edge | R-07 finding, C2 |
| NFR1.13 | Per-message/per-user budget enforcement is **deferred to U2** (FR6.7) — the skeleton records this as an accepted gap, not a silent omission | Q3, FR6.7 |

## Privacy disclosure

| id | requirement | basis |
|----|-------------|-------|
| NFR2.1 | `docs/CONFIGURATION.md`'s oxpilot section discloses plaintext transcript storage + redact-on-write + per-user isolation — the NFR2 plaintext-disclosure half ships with the env docs (BR1.5), not silently | NFR2, Q7-decided (inception), R-08 finding |

## Threat considerations

STRIDE-lite for the skeleton: **Spoofing** — session-cookie auth only,
no bearer ambiguity (NFR1.1–1.3). **Tampering** — anti-enumeration +
ownership filter on every query (NFR1.4). **Repudiation** — 
session-create audit events; delete's audit lands with U2's proc
(NFR5 share). **Information disclosure** —
gate-fail-closed + status payload trim + redaction + no-secret-surface
(NFR1.5–1.9). **Denial of service** — one-live-turn-per-session bound
(NFR1.10) + body cap (NFR1.12); message budgets explicitly deferred
(NFR1.13). **Elevation** —
no role surface exists in U1; SessionOnly classification blocks the
PAT/MCP privilege path entirely.

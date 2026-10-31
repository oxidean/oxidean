# Security Requirements — session-core (U2)

Consumes: `construction/session-core/functional-design/rules.md`
(BR1.1–BR7.3), `functional-design/entities.md`,
`inception/requirements-analysis/requirements.md` (NFR1, NFR2, NFR5),
`inception/contract-design/contract-summary.md` (C1, C2, C5, C8),
`codekb/oxidean-app/technology-stack.md`.

## Authentication & authorization

| id | requirement | basis |
|----|-------------|-------|
| NFR1.1 | Every `SessionService` method takes the caller `user_id` — session rows are always read/written under `WHERE user_id = $caller`; the request body carries NO ownership field and write rows bind `caller.user_id` unconditionally | NFR1, BR1.1, BR1.2, C5 |
| NFR1.2 | Missing AND cross-user rows both resolve to `not_found` — the service exposes no distinguishable `forbidden` result that would let an actor probe for another user's session ids; the C5 prose "forbidden" pins to the anti-enumeration `not_found` (fd review R-11 refinement) | NFR1/NFR2, BR1.1, C1 |
| NFR1.3 | The U2 procs (`session_get`, `session_delete`, `message_queue`, `queue_list`, `usage_get`) are SessionOnly + `oxpilot.disabled`-gated like `session_create`; `oxpilot.status` remains the sole gate-exempt proc; bearer/PAT actors get `auth.pat_scope`, unauthenticated `auth.unauthenticated` | NFR1, BR1.3 |
| NFR1.4 | The service layer enforces scoping independently of the proc layer — a buggy or future non-proc caller (U3 runtime, U6 gateway) cannot bypass `user_id` binding because the store layer itself takes the caller id | BR1.1, C5 |

## Secret custody & redaction

| id | requirement | basis |
|----|-------------|-------|
| NFR1.5 | `oxidean_core::redact()` runs at the service boundary on EVERY body-bearing write — message `content`, queued `content`, tool-call `args_summary`/`output_summary`, proposal `title` and `diff_body`, session `goal`/`title` — before any bytes reach `oxidean-db`. Nothing leaves the service unredacted (C5 rule). The redact set is exactly BR3.1's — identifier fields like proposal `base` (a ref/sha) and `client_msg_id` are NOT redacted: a legitimately secret-shaped ref would be corrupted, and ids carry no user content | NFR1, BR3.1, AC5.3.3 |
| NFR1.6 | Stored transcript/queue/tool/proposal content is post-redaction plaintext — the disclosed at-rest model (NFR2); no raw-input retention path, no double-store of pre-redaction bytes, no logging of content at any level | NFR1/NFR2, BR3.1 |
| NFR1.7 | `client_msg_id` values transit the service opaquely — they are caller-generated handles carried into the transcript row for dedupe; the service never logs, audits, or redacts them (they are ids, not content) | BR2.2 |

## Surface integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR1.8 | `session_delete` is a destructive op — hooks run in registration order BEFORE the single cascade; a hook failure aborts deletion with the row intact (C8). The cascade itself is one statement so a partial delete is impossible | BR5.1, BR5.2, C8, AC1.5.2 |
| NFR1.9 | `SessionDeleteHook` registrars are authoritative for their own cleanup — the service can only invoke them; U2 ships zero production registrars, so nothing extra fires in v1 | BR5.2, C8 |
| NFR1.10 | `usage_get` returns only the caller's own counters — scope_key IS the user_id for user scope; the instance scope row exposes aggregate counts, not another user's usage | NFR2, BR6.1 |

## Threat considerations

STRIDE-lite for session-core: **Spoofing** — SessionOnly + caller-scoped
service (NFR1.1–1.4). **Tampering** — atomic queue drain (one tx),
single-statement cascade, monotonic `seq`/`queue_seq` minted per session
(NFR1.8; reliability-requirements NFR4.x). **Repudiation** — audit row
on `session_delete` with field-names only (observability-requirements
NFR5.1). **Information disclosure** — `not_found` anti-enumeration,
redact-on-write, disclosed plaintext model, no content logging
(NFR1.2, NFR1.5–1.7). **Denial of service** — bounded transcript reads,
index-covered queue scans, `budget_exceeded` refusal
(performance-requirements NFR3.x, NFR6.x). **Elevation of privilege** —
no admin transcript-reader path in v1; scoping can't be relaxed by any
proc parameter (NFR1.1–1.4).

## Privacy disclosure

| id | requirement | basis |
|----|-------------|-------|
| NFR2.1 | `docs/CONFIGURATION.md`'s oxpilot section already discloses transcript-at-rest handling; U2 extends it where needed with: queued-message at-rest disclosure (same redact-then-store model as transcripts) and the budget-counter scope (per-user + per-instance UTC-day counters, no content stored) | NFR2, BR3.1, BR6.1 |

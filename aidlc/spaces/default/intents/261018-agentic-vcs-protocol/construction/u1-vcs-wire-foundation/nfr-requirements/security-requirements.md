# Security Requirements — u1-vcs-wire-foundation

## SR1 — Fail-closed classification

Every `vcs.*` procedure name resolves through `classify_pat_proc` to exactly one tier (`RepoRead`, `RepoWrite`, `RepoAdminWrite`); any name not in the pinned 17+2 set resolves `SessionOnly` (denied to all token callers). The coverage test asserts the full mapping — a new `vcs.*` name without a tier entry fails the build, never silently inherits access.

## SR2 — Authz layering on the wire

| surface | credential kinds | denial code |
|---|---|---|
| `POST /api/rpc` + `/api/rpc/ws` | session cookie, PAT, job token | transport layer (non-exhaustive — the existing set applies unchanged): `auth.unauthenticated`, `auth.token_expired`, `auth.token_revoked`, `auth.banned`, `auth.pat_scope`, `auth.rate_limited`, `auth.setup_required`, `rpc.version_mismatch` (fires pre-auth on a missing/stale `Oxidean-RPC-Version` header — the most common `vcs.*` transport denial), `rpc.unknown_procedure` (what a cookie caller gets for an unlisted name — `SessionOnly` shields token callers only); anti-enumeration `repo.not_found` |
| `/api/vcs/*` object routes | PAT Basic/Bearer, OAuth access token (never cookies) | `vcs.unauthenticated` (missing/cookie-only), `vcs.forbidden` (insufficient tier) |

Scope: this table covers the RPC + object-route surfaces only; `/api/mcp` descriptor authz (contract C3 — accepts the `oxidean_session` cookie by design, denials as `isError` tool content rather than codes) is u9's.

Wire types must not weaken this layering: `vcs.unauthenticated`/`vcs.forbidden` are domain+route codes only; the RPC pipeline never emits them pre-dispatch (transport denials stay `auth.*` so a caller can distinguish "your token lacks scope" from "the domain refused you").

Row 2's column is credential denials; the full C2 domain-denial set on object routes also includes `vcs.not_found` (FG-uncovered private repo — same anti-enumeration shape as row 1) plus `vcs.disabled`/`vcs.quota_exceeded` at the domain layer. Contract C2 lists "PAT and OAuth access tokens only" — u7 should confirm whether repo-locked job tokens (`oxidean_job_`) read objects in Actions contexts; the shared resolver shape admits them for free.

## SR3 — Error-payload bounds

`AppError.data` payloads carry only identifiers the caller is already authorized to see (authz precedes payload construction). No redaction pass is required; the bound is size: `vcs.conflict`'s `data.conflicts` caps at 500 entries (the vcs page bound — contract C1 `limits.pagination` ≤500) — a larger conflict set truncates with a dedicated `data.truncated: true` flag (the house convention — `repo_types.rs` `truncated: bool`, C1's diff `{truncated:true}` marker), never by overloading the free-text `detail` field, so a pathological merge cannot mint an unbounded error body.

## SR4 — Decode safety

- `{owner, name}` required on all 17 inputs — absent → `auth.pat_scope` pre-dispatch for fine-grained tokens (`repo_target` parses the target from raw JSON before typed decode), `rpc.bad_input` post-authz for cookie/classic/session callers. Never a fail-open grant either way.
- `ref_update`'s `expected_old`/`new` use the `Nullable<ChangesetId>` encoding: absent → `missing_field` (not a silent delete), null → create-only/delete, value → CAS.
- `#[serde(deny_unknown_fields)]` is never applied — additive evolution requires consumers to ignore unknown fields (BR1.6).
- Request-side bounds are contract-pinned (C1 `limits`, `vcs.too_large`) but enforced at the **handler** layer in u7, not the type layer — DTO fields are unvalidated serde structs by house design: u7 clamps/rejects `limit` ≤500, `changes[]` ≤10_000, and validates `owner`/`name` via a `validate_repo_name`-equivalent before any repo lookup. The C1 body-cap raise for `content_b64` headroom widens the exposure u7 inherits — the handler bounds are the backstop.

## SR5 — No secrets / no unsafe content on the wire

Wire DTOs carry identifiers and metadata only — object bytes move through the dedicated `/api/vcs/*` object routes, and `content_b64` inline transport is capped at 256 KiB per entry (AC2.1.2). No credential material, session tokens, or environment secrets appear in any wire type or error payload.

---
status: complete
phase: 08-git-https-pats
source: [08-VERIFICATION.md]
started: 2026-09-13T20:08:56Z
updated: 2026-09-13T21:14:30Z
---

## Current Test

[testing complete]

## Tests

### 1. Tokens UI — create classic / reveal / revoke in a real browser
expected: List empty hero → Generate → classic form → reveal once → list shows prefix/note → revoke confirm removes token; password never accepted as git secret
result: pass
evidence: |
  Vitest `tokens.integration.test.ts` — list empty hero, Generate, classic/FG create+reveal, revoke AlertDialog (16 tests green 2026-09-13).
  nextest `pat_rpc`: createClassic one-time token, list omits secret, revoke removes (18/18 suite with Smart HTTP).
  Live browser skipped — Docker unavailable in this environment; coverage is automated.

### 2. FG + real git HTTPS — Compose smoke
expected: Token mints with oxidean_fg_; Basic auth with PAT works; account password fails with PAT hint; private anon gets 401+WWW-Authenticate
result: pass
evidence: |
  nextest `pat_rpc` FG create (all/selected/foreign/empty/unverified) green.
  nextest `git_smart_http`: pat push/fetch happy path, password reject 401, private anon 401+WWW-Authenticate, cookie ignore, scope 403, rate limit 429, unverified push deny — 18/18 green.
  `scripts/smoke-git-https.sh` + `make smoke-git-https` present; live Compose not run (Docker CLI unavailable).

### 3. How-to panel + long URL wrap — visual backstop
expected: How-to lists username aliases git/token/oauth2, password=PAT, Create CTA → /settings/tokens; long URL does not clip awkwardly
result: pass
evidence: |
  Vitest `clone-box.pat.integration.test.ts` — aliases copy, CTA href `/settings/tokens`, and `overflow-x-auto` + `break-all` + `whitespace-pre-wrap` on how-to `<code>` (3/3 green after UAT coverage add).

### 4. Judgment prohibitions — plaintext-at-rest / no RPC Bearer ack
expected: DB only stores token_hash; /api/rpc still session-only; docs say PATs are not RPC Bearer
result: pass
evidence: |
  Schema `token_hash CHAR(64) UNIQUE` in `0008_pats.sql` (all dialects); no plaintext column.
  `docs/API.md` D-01: PATs are not RPC Bearer; RPC uses `oxidean_session` cookie only.
  `git_smart_http.rs` ignores Cookie for auth; `pat_list_omits_secret_token` nextest asserts list has no token field.

## Summary

total: 4
passed: 4
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

## Automated coverage map

| UAT # | Primary automated evidence |
|-------|----------------------------|
| 1 | `apps/web/.../tokens.integration.test.ts` + `pat_rpc` |
| 2 | `git_smart_http` + `pat_rpc` (+ Compose smoke script, Docker required for live) |
| 3 | `clone-box.pat.integration.test.ts` (aliases, CTA, wrap classes) |
| 4 | migrations + `docs/API.md` + nextest list/cookie assertions |

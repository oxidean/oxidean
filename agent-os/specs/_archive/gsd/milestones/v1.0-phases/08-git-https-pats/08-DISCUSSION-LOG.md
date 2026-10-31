# Phase 8: Git HTTPS & PATs - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-13
**Phase:** 08-Git HTTPS & PATs
**Areas discussed:** PAT capabilities, HTTPS credential contract, Token management UX, Smart HTTP surface, Verify gate for PATs, Auth abuse limits, Clone URL host, Fine-grained all repos

---

## PAT capabilities

| Option | Description | Selected |
|--------|-------------|----------|
| Git only | HTTPS git; no RPC Bearer | ✓ |
| Git + RPC | PAT also for typed RPC | |
| You decide | | |

**User's choice:** Git only; support git and HTTPS like GitHub (SSH later as other forges do)
**Notes:** Dual-remote mental model; SSH stays Phase 9

| Option | Description | Selected |
|--------|-------------|----------|
| Single full git access | Any PAT full write where permitted | |
| Classic scopes | read vs write | |
| You decide + both classic & FG | | ✓ |

**User's choice:** Support both classic scopes and fine-grained scopes
**Notes:** Pulls PAT fine-grained into Phase 8 (OAuth apps remain later)

| Option | Description | Selected |
|--------|-------------|----------|
| Selected repos only | | |
| Repos + account toggles | | |
| You decide — forge parity | | ✓ |

**User's choice:** Match GitHub and other forges
**Notes:** Exact scope catalog → Claude discretion / research

| Option | Description | Selected |
|--------|-------------|----------|
| Two create flows | Classic vs Fine-grained separate | ✓ |
| One wizard | | |
| You decide | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Optional expiry | | ✓ |
| Required expiry | | |
| You decide | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Prefixed opaque | Oxidean prefixes | ✓ |
| Unprefixed | | |
| You decide | | |

**Notes:** No “github” / `gh*` in prefix

| Option | Description | Selected |
|--------|-------------|----------|
| Last-used yes | | ✓ |
| Created/expires only | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Timestamp + IP | | ✓ |
| Timestamp only | | |

---

## HTTPS credential contract

| Option | Description | Selected |
|--------|-------------|----------|
| Username + PAT as password | Forge pattern | ✓ |
| Token as username | | |
| You decide | | |

**Notes:** Match GitHub and other forges

| Option | Description | Selected |
|--------|-------------|----------|
| Hard reject password | | |
| Hard reject + hint | | ✓ |
| You decide | | |

| Option | Description | Selected |
|--------|-------------|----------|
| PAT-only (no cookies) | | ✓ |
| PAT or session | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Short inline hint | | |
| Full how-to panel | | ✓ |
| You decide | | |

---

## Token management UX

| Option | Description | Selected |
|--------|-------------|----------|
| `/settings/tokens` | | ✓ |
| Under profile settings | | |
| You decide | | |

| Option | Description | Selected |
|--------|-------------|----------|
| One-time reveal | | ✓ |
| One-time + download | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Required note | | ✓ |
| Optional note | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Confirm on revoke | | ✓ |
| Immediate revoke | | |

---

## Smart HTTP surface

| Option | Description | Selected |
|--------|-------------|----------|
| `/{owner}/{repo}.git` same origin | | ✓ |
| Prefixed `/git/...` | | |
| You decide | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Anonymous public read | | ✓ |
| Always require PAT | | |

| Option | Description | Selected |
|--------|-------------|----------|
| 401 + WWW-Authenticate | Private unauth | ✓ |
| 404 like web | | |
| You decide | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Require `.git` suffix | | ✓ |
| Both with/without `.git` | | |

| Option | Description | Selected |
|--------|-------------|----------|
| 403 insufficient scope | | ✓ |
| 401 | | |

---

## Verify gate for PATs

| Option | Description | Selected |
|--------|-------------|----------|
| Verified for create + push | | ✓ |
| Verified to create only | | |
| No verify gate | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Same as privileged (`auth.email_unverified`) | | ✓ |
| Generic 403 | | |

---

## Auth abuse limits

| Option | Description | Selected |
|--------|-------------|----------|
| Per IP and per user | | ✓ |
| Per IP only | | |
| No special limit | | |

| Option | Description | Selected |
|--------|-------------|----------|
| 429 + Retry-After | | ✓ |
| 403 only | | |

---

## Clone URL host

| Option | Description | Selected |
|--------|-------------|----------|
| `OXIDEAN_PUBLIC_ORIGIN` | | ✓ |
| Request Host | | |
| Origin then Host fallback | | |

---

## Fine-grained all repos

| Option | Description | Selected |
|--------|-------------|----------|
| Selected only | | |
| All current + future | | ✓ |
| All current only | | |

---

## Claude's Discretion

- Exact classic / fine-grained scope catalogs (forge parity)
- Exact Basic-auth username aliases
- Exact rate-limit thresholds
- Smart HTTP implementation approach and Traefik routing details

## Deferred Ideas

- SSH (Phase 9)
- PAT as RPC Bearer (out of Phase 8)
- OAuth apps (remaining PLAT-V2-02)
- Org-scoped tokens (Phase 10+)

---

## 08-02 Reversibility gates (checkpoint outcomes)

### Task 1 — D-08 PAT prefixes (one-way)

| Option | Description | Selected |
|--------|-------------|----------|
| `ona_prefixes` | `ona_pat_` / `ona_fg_` (plan recommended; CONTEXT/RESEARCH) | |
| `oxidean_prefixes` | `oxidean_pat_` / `oxidean_fg_` (full-brand) | ✓ |
| `stop` | Revisit prefix design | |

**User's choice:** `oxidean_prefixes` — classic `oxidean_pat_`, fine-grained `oxidean_fg_`, then CSPRNG hex (32+ bytes). Never `ghp_` / `github_pat_` / `gho_` / any github/gh* prefix.
**Notes:** Deviation from plan option id `ona_prefixes`. CONTEXT/RESEARCH recommended shorter `ona_*` brand prefixes; human locked full-brand `oxidean_*` as the public token format / secret-scanning contract (D-08 one-way).
**Recorded:** 2026-09-13 (08-02 continuation)

### Task 2 — D-18 HTTPS clone URL (one-way)

| Option | Description | Selected |
|--------|-------------|----------|
| `owner_repo_git` | `https://{host}/{owner}/{repo}.git` on public origin (recommended — D-18) | ✓ |
| `stop` | Revisit URL scheme | |

**User's choice:** `owner_repo_git` — public clone URL shape is `https://{host}/{owner}/{repo}.git` on the same public origin (host from `OXIDEAN_PUBLIC_ORIGIN`, D-18/D-19). Smart HTTP only on `/{owner}/{repo}.git`; bare `/{owner}/{repo}` stays web UI (D-22).
**Notes:** Matches Phase 7 clone box; Traefik PathRegexp must keep SPA from stealing `.git`.
**Recorded:** 2026-09-13 (08-02 continuation)

### Task 3 — D-21 git private unauth 401 + D-01 PAT-not-RPC (costly)

| Option | Description | Selected |
|--------|-------------|----------|
| `git_401_pat_https_only` | 401 + WWW-Authenticate Basic for private/no-access git; PATs HTTPS-git-only (not RPC Bearer) | ✓ |
| `stop` | Revisit error/auth class | |

**User's choice:** `git_401_pat_https_only` — unauthenticated git access to private/no-access repos returns **401 + WWW-Authenticate Basic** (not web `repo.not_found` 404). PATs authenticate **HTTPS git only** in Phase 8 — not typed RPC Bearer; web stays on session cookies (D-01, D-21). GIT-11 ROADMAP parenthetical “RPC/API where applicable” interpreted as HTTPS-git-only for Phase 8.
**Notes:** Intentional git-vs-web split; credential helpers expect 401 challenges. Mixing 404 with git confuses clients.
**Recorded:** 2026-09-13 (08-02 continuation)

---

## 08-03 Schema proceed gate (checkpoint outcome)

### Task 0 — Confirm proceed with D-08/D-18/D-21 from 08-02

| Option | Description | Selected |
|--------|-------------|----------|
| `proceed_locked` | Proceed with locked D-08/D-18/D-21 (recommended) | ✓ |
| `stop` | Revisit 08-02 | |

**User's choice:** `proceed_locked` — implement PAT schema/types/CRUD using locked contracts from 08-02:
- **D-08:** `oxidean_pat_` / `oxidean_fg_` (option `oxidean_prefixes`; **not** plan option id `ona_prefixes` / `ona_pat_`/`ona_fg_`)
- **D-18:** `owner_repo_git` — `/{owner}/{repo}.git`
- **D-21/D-01:** `git_401_pat_https_only`

**Notes:** Plan 08-03 checkpoint text still says `ona_prefixes`; human + 08-02 SUMMARY override to full-brand `oxidean_*` everywhere token prefixes appear in new code.
**Recorded:** 2026-09-13 (08-03 continuation)

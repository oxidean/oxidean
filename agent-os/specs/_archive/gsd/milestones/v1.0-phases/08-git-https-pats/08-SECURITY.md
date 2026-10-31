---
phase: "08"
slug: "git-https-pats"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-09-13"
---

# Phase 08 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Web session ↔ typed RPC | Browser cookies authenticate `/api/rpc` only | Session id; never PAT Bearer |
| Git client ↔ Smart HTTP | Basic auth username+PAT over `/{owner}/{repo}.git` | PAT secret (once); token_hash lookup |
| PAT mint ↔ DB | Create returns plaintext once; persist hash only | `oxidean_pat_`/`oxidean_fg_` secret → SHA-256 hex |
| Traefik ↔ API vs SPA | `.git` PathRegexp priority 110 → API; bare paths → web | Clone/push traffic vs UI HTML |
| Trusted proxy ↔ rate limit | Rightmost `X-Forwarded-For` hop for failed-auth IP bucket | Client IP identity |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-08-01 | Information disclosure | PAT create/list/reveal/DB | high | mitigate | One-time plaintext; SHA-256 hash-at-rest; list prefix+fingerprint only (D-15) | closed |
| T-08-02 | Elevation of privilege | Smart HTTP Basic / receive-pack | high | mitigate | PAT-prefix/hash only; reject passwords; ignore cookies; verified+scope for push (D-11/D-12/D-20/D-23/D-24) | closed |
| T-08-03 | Elevation of privilege | Tokens UI Generate/create | medium | mitigate | AuthShell / disable Generate when !email_verified (D-24/D-25); server `require_verified` | closed |
| T-08-04 | Spoofing | Token prefixes | high | mitigate | Locked `oxidean_pat_` / `oxidean_fg_` (not `ona_*` / `gh*`) — D-08 | closed |
| T-08-05 | Information disclosure | Private git status | medium | mitigate | Unauth private → 401+WWW-Authenticate, not web 404 (D-21) | closed |
| T-08-06 | Elevation of privilege | FG selected repos | high | mitigate | Ownership check + join table; dedupe; transactional create | closed |
| T-08-07 | Tampering | git-http-backend CGI | high | mitigate | argv to binary; `bare_repo_path` validation; `GIT_HTTP_EXPORT_ALL` | closed |
| T-08-08 | Denial of service | Failed auth rate limit | high | mitigate | 20/IP + 10/user / 15m → 429; rightmost XFF (D-26) | closed |
| T-08-09 | Spoofing | Traefik `.git` routing | high | mitigate | PathRegexp priority 110 to API; smoke rejects `text/html` | closed |
| T-08-10 | Spoofing | Docs/client misuse | medium | mitigate | Document PAT HTTPS-only; cookies ignored on `.git` (D-01/D-12) | closed |
| T-08-SC | Tampering | packages | low | accept | No new crates.io/npm packages for phase implementation | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-08-SC | T-08-SC | Phase ships PAT/Smart HTTP using existing workspace crates (sqlx, axum, tokio process). No new crates.io/npm packages; in-process failed-auth limiter. Package-supply risk unchanged from prior phases. | plan disposition + secure-phase audit | 2026-09-13 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-09-13 | 11 | 11 | 0 | gsd-security-auditor (ASVS L1) |

### Security Audit 2026-09-13

| Metric | Count |
|--------|-------|
| Threats found | 11 |
| Closed | 11 |
| Open (blocking ≥ high) | 0 |
| Accepted documented | 1 (T-08-SC) |

Evidence highlights: `pat/mod.rs` hash-at-rest + one-time create; `git_smart_http.rs` Basic/PAT/cookie-ignore/401/rate-limit; `pats.rs` transactional FG links; `pat_types.rs` `oxidean_*` prefixes; Traefik `docker-compose.yml` priority 110; docs API/CONFIGURATION PAT HTTPS-only. Code-review fixes WR-01…WR-04 + IN-01…IN-03 incorporated before this audit.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-09-13

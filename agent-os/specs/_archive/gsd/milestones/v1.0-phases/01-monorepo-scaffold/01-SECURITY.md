---
phase: 1
slug: monorepo-scaffold
status: verified
threats_open: 0
asvs_level: 1
created: 2026-09-09
---

# Phase 1 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Developer workstation → git repo | Scaffold must not commit secrets | `.env`, credentials |
| Browser/client → `/api/rpc` + `/api/rpc/ws` | Untrusted JSON/WS; public in Phase 1 | RPC payloads, version header |
| Env `OXIDEAN_CORS_ORIGINS` → CORS layer | Misconfig can open origins | Origin allowlist |
| Internet/host → Traefik → web/api | All Compose ingress | HTTP/WS |
| Compose env files | DB password and CORS | Secrets via `.env` |
| PR fork → CI | Untrusted code in CI | No production secrets |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-01-01 | Information disclosure | `.env.example` | mitigate | `.env` / `.env.local` in `.gitignore`; `.env.example` documents placeholders only | closed |
| T-01-02 | Tampering | lockfiles | accept | Phase 1 accepts lockfile churn; CI uses `bun install --frozen-lockfile` | closed |
| T-01-03 | Spoofing | RPC version header | mitigate | `Oxidean-RPC-Version` required on HTTP+WS; `rpc.version_mismatch` on miss/mismatch (`app.rs`, `rpc.rs`, tests) | closed |
| T-01-04 | Information disclosure | CORS | mitigate | Dev mirrors request origin; non-dev requires non-empty `OXIDEAN_CORS_ORIGINS` or API refuses start (`cors.rs` + unit tests) | closed |
| T-01-05 | Denial of service | public echo | accept | No auth/rate-limit in Phase 1; echo capped at `ECHO_MAX_BYTES` (8192) | closed |
| T-01-06 | Tampering | generated client | mitigate | `scripts/check-rpc-sync.sh` + `make rpc-sync-check` | closed |
| T-01-07 | XSS | status/error rendering | mitigate | Status page renders strings as text; no `dangerouslySetInnerHTML` in `apps/web/src` | closed |
| T-01-08 | Spoofing | Auth placeholders | accept | Sign in/Sign up disabled; no fake session cookies | closed |
| T-01-09 | Information disclosure | compose DB password | mitigate | `.env` gitignored; `.env.example` local-only `oxidean` password | closed |
| T-01-10 | Spoofing | CORS in Compose | mitigate | Compose defaults `OXIDEAN_ENV=compose` + `OXIDEAN_CORS_ORIGINS` Traefik origins | closed |
| T-01-11 | Denial of service | exposed ports | accept | Local-dev binding; README warns not to expose Compose to WAN without auth | closed |
| T-01-12 | Elevation | CI secrets | mitigate | `.github/workflows/ci.yml` uses no production secrets / `secrets.*` | closed |
| T-01-13 | Tampering | generated client (CI) | mitigate | CI `rpc-sync` job runs `make rpc-sync-check` | closed |

*Status: open · closed*
*Disposition: mitigate · accept · transfer*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01-02 | T-01-02 | Phase 1 lockfile churn acceptable; frozen install enforced in CI | secure-phase audit | 2026-09-09 |
| AR-01-05 | T-01-05 | Public unauthenticated `system.echo` without rate limits until auth phases; 8KiB cap only | secure-phase audit | 2026-09-09 |
| AR-01-08 | T-01-08 | Non-functional auth chrome is intentional until Phase 4 | secure-phase audit | 2026-09-09 |
| AR-01-11 | T-01-11 | Local Compose exposure without auth is operator responsibility; documented in README | secure-phase audit | 2026-09-09 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-09-09 | 13 | 13 | 0 | gsd-secure-phase (agent verification) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-09-09

---
phase: 2
slug: multi-db-storage
status: verified
threats_open: 0
asvs_level: 1
created: 2026-09-09
---

# Phase 2 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Operator env → `oxidean-db` | `DATABASE_URL` / `OXIDEAN_DB_DIALECT` may contain credentials | Connection strings |
| `oxidean-db` → Postgres / MySQL / SQLite | SQL + bound params | App schema / probe row |
| Host filesystem `./var` → SQLite file | Runtime DB file + WAL/SHM | Local file state |
| Client → `/api/rpc` `system.db_probe` | Unauthenticated in Phase 2 | Probe response dialect/count |
| Smoke/switch scripts → operator terminal | Must not echo secrets | Dialect labels only |
| Fork PR → CI `db-matrix` | Untrusted code; throwaway local DBs | No production secrets |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-02-01 | Information disclosure | connect/migrate/probe/CLI errors | mitigate | `redact_url()` on error/CLI output; unit test `redact_url_hides_password` | closed |
| T-02-02 | Tampering | per-dialect migration drift | mitigate | `migration_parity` unit test across `migrations/{postgres,mysql,sqlite}` | closed |
| T-02-03 | Tampering | SQL injection in probe | mitigate | Fixed SQL per dialect with bound placeholders only (`$1` / `?` / `?1`) | closed |
| T-02-04 | Information disclosure | SQLite file on disk | accept | `var/` gitignored; no secrets in Phase 2 schema; ops hardening deferred | closed |
| T-02-05 | Denial of service | connection pool exhaustion | mitigate | `max_connections(5)` PG/MySQL; `max_connections(1)` SQLite | closed |
| T-02-06 | Information disclosure | `system.db_probe` errors | mitigate | Wire code `db.probe_failed` + generic message; detail only in logs (`rpc.rs` + tests) | closed |
| T-02-07 | Denial of service | unauthenticated probe writes | accept | Single-row upsert `id=1`; auth gate deferred to system dashboard | closed |
| T-02-08 | Tampering | auto-migrate on boot | mitigate | `OXIDEAN_AUTO_MIGRATE=false` skips boot migrate; failure aborts startup | closed |
| T-02-09 | Spoofing | wrong-DB attribution | mitigate | Response `dialect` read back from `instances` row, not env alone | closed |
| T-02-10 | Tampering | generated client drift | mitigate | `make rpc-sync-check` / CI `rpc-sync` covers `system.dbProbe` | closed |
| T-02-11 | Information disclosure | SQLite committed to git | mitigate | `.gitignore` includes `var/` | closed |
| T-02-12 | Information disclosure | scripts echo credentials | mitigate | Switch/smoke print dialect/status only; never echo `DATABASE_URL` value | closed |
| T-02-13 | Tampering | migrate populated DB | mitigate | `migrate --assert-empty` + `db-switch-dialect.sh` refuse non-empty targets | closed |
| T-02-14 | Spoofing | smoke wrong dialect | mitigate | Smoke asserts `"dialect":"<expected>"` from `system.db_probe` body | closed |
| T-02-15 | Denial of service | stale compose overlays | accept | Local-only; smoke traps `compose down --remove-orphans` | closed |
| T-02-16 | Elevation | CI secrets to forks | mitigate | `db-matrix` uses no `secrets.*`; throwaway `oxidean` credentials only | closed |
| T-02-17 | Information disclosure | CI URL logs | accept | Matrix URLs are non-secret localhost throwaways; app redacts passwords | closed |
| T-02-18 | Information disclosure | weak prod credentials in docs | mitigate | `docs/database.md` / README / `.env.example` label local-only; warn on exposure | closed |
| T-02-19 | Tampering | undocumented destructive switch | mitigate | Docs: empty-target-only switch; no cross-dialect data copy | closed |

*Status: open · closed*
*Disposition: mitigate · accept · transfer*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-02-04 | T-02-04 | SQLite file permissions inherit umask; Phase 2 schema has no secrets; filesystem ACL hardening is ops | secure-phase audit | 2026-09-09 |
| AR-02-07 | T-02-07 | Unauthenticated `system.db_probe` is a bounded single-row upsert; admin auth deferred to dashboard phases (CONTEXT) | secure-phase audit | 2026-09-09 |
| AR-02-15 | T-02-15 | Stale compose resources are local-dev only; smoke cleanup trap mitigates common case | secure-phase audit | 2026-09-09 |
| AR-02-17 | T-02-17 | CI matrix DATABASE_URL values are public throwaways on localhost services | secure-phase audit | 2026-09-09 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-09-09 | 19 | 19 | 0 | gsd-secure-phase (agent verification) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-09-09

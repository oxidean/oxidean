---
phase: "20"
slug: "packages-registry"
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-14"
updated: "2026-09-19"
validated_at: "2026-09-19"
---

# Phase 20 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by plan-phase from 20-RESEARCH.md Validation Architecture. Nyquist reconcile via `/gsd-validate-phase` equivalent (22.1-09, 2026-09-19).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust: cargo-nextest / `cargo test`; Web: Vitest via Bun |
| **Config file** | `.config/nextest.toml`; `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(oci_registry)|test(npm_registry)|test(generic_registry)|test(package_acl)|test(package_rpc)|test(package_quota)|test(package_gc)'` |
| **Full suite command** | `make test` (+ `make rpc-sync-check`; `make smoke-packages` skip-ok without Docker) |
| **Estimated runtime** | ~90–240 seconds (quick); full suite longer with e2e/smoke |

---

## Sampling Rate

- **After every task commit:** targeted nextest filter for touched protocol or Vitest for UI plans
- **After every plan wave:** package nextest + web packages Vitest + `make rpc-sync-check` after RPC changes
- **Before `/gsd-verify-work`:** `make test` + `make smoke-packages` (skip-ok) green
- **Max feedback latency:** 240 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 20-00-T1 | 00 | 0 | PKG-01..05 | T-20-SC | Wave 0 RED Rust stubs | integration | nextest list package filters | ✅ | ✅ green |
| 20-00-T2 | 00 | 0 | PKG-01..05 | T-20-SC | dialect_packages stub | unit | dialect_packages | ✅ | ✅ green |
| 20-01-T1 | 01 | 0 | PKG-05 | T-20-SC | Wave 0 web + smoke stubs | component/smoke | Vitest packages + smoke-packages.sh | ✅ | ✅ green |
| 20-02-T* | 02 | 1 | PKG-04 | T-20-01 | Schema + edge routing + reserved slugs | integration | dialect_packages + Traefik PathPrefix | ✅ | ✅ green |
| 20-03-T* | 03 | 2 | PKG-04 | T-20-02,T-20-03 | Store + ACL∩PAT scopes; cookies ignored | integration | `test(package_acl)` | ✅ | ✅ green |
| 20-04-T* | 04 | 3 | PKG-03 | T-20-04 | Tracer generic PUT/GET/DELETE + mounts | integration | `test(generic_registry)` | ✅ | ✅ green |
| 20-05-T* | 05 | 4 | PKG-01 | T-20-05 | OCI push/pull + Bearer realm | integration | `test(oci_registry)` | ✅ | ✅ green |
| 20-06-T* | 06 | 4 | PKG-02 | T-20-06 | npm publish + tarball install | integration | `test(npm_registry)` | ✅ | ✅ green |
| 20-07-T* | 07 | 5 | PKG-02 | T-20-06 | dist-tags / deprecate / search | integration | `test(npm_registry)` | ✅ | ✅ green |
| 20-08-T* | 08 | 5 | PKG-05 | T-20-07 | Session RPC list/delete Admin | integration | `test(package_rpc)` | ✅ | ✅ green |
| 20-09-T* | 09 | 6 | PKG-01..03 | T-20-08 | Quotas reject + GC refcount-safe | integration | `test(package_quota)\|test(package_gc)` | ✅ | ✅ green |
| 20-10-T* | 10 | 6 | PKG-05 | T-20-07 | Owner/repo packages UI + type-to-confirm | component | Vitest owner/repo packages | ✅ | ✅ green |
| 20-11-T* | 11 | 7 | PKG-04,PKG-05 | T-20-02,T-20-08 | Admin quota + tokens package scopes UI | component | Vitest admin + tokens.packages | ✅ | ✅ green |
| 20-12-T* | 12 | 8 | PKG-01..05 | T-20-01 | Docs + smoke + phase gate | mixed | `make smoke-packages` + rpc-sync-check + docs rg | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/oxidean-api/tests/oci_registry.rs` — PKG-01 (20-00 → greened 20-05)
- [x] `crates/oxidean-api/tests/npm_registry.rs` — PKG-02 (20-00 → greened 20-06/07)
- [x] `crates/oxidean-api/tests/generic_registry.rs` — PKG-03 (20-00 → greened 20-04)
- [x] `crates/oxidean-api/tests/package_acl.rs` — PKG-04 (20-00 → greened 20-03)
- [x] `crates/oxidean-api/tests/package_rpc.rs` — PKG-05 (20-00 → greened 20-08/09)
- [x] `crates/oxidean-db/tests/dialect_packages.rs` — packages migration parity (20-00 → greened 20-02)
- [x] `apps/web/src/routes/$owner.packages.integration.test.ts` — owner packages UI (20-01 → greened 20-10)
- [x] `apps/web/src/routes/$owner.$repo.packages.integration.test.ts` — repo-linked packages (20-01 → greened 20-10)
- [x] `apps/web/src/routes/admin/packages.integration.test.ts` — Admin quota (20-01 → greened 20-11)
- [x] `apps/web/src/routes/settings/tokens.packages.integration.test.ts` — package scope tokens (20-01 → greened 20-11)
- [x] `scripts/smoke-packages.sh` + `make smoke-packages` — edge routing smoke (20-01 → greened 20-12)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `docker login` + `docker push/pull` against `/v2` | PKG-01 | Docker engine may be absent in CI | When Docker available: login with PAT-as-password; push `{host}/{owner}/{image}:tag`; pull anonymous if public |
| `npm publish` / `npm install` against `/npm/{owner}/` | PKG-02 | Client UX + registry config | Configure registry to `/npm/{owner}/`; publish then install in clean dir |
| Type-to-confirm delete dialog | PKG-05 | Browser interaction | Delete flow requires typing `name@version`; Cancel leaves version; Confirm removes |
| Admin quota usage chart readability | PKG-05 / ops | Visual | Admin packages page shows format + package breakdown vs env defaults |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 240s
- [x] `nyquist_compliant: true` set in frontmatter — owned by `/gsd-validate-phase` (set 2026-09-19 / 22.1-09)

**Approval:** validated (Nyquist compliant) — API package suite green; Docker/npm client + delete dialog remain manual backstops

---

## Validation Audit 2026-09-19

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |
| Manual-only (client UX) | 4 |

| Gate | Result |
|------|--------|
| `cargo nextest run -p oxidean-api -E 'test(oci_registry)\|test(npm_registry)\|test(generic_registry)\|test(package_acl)\|test(package_rpc)\|test(package_quota)\|test(package_gc)'` | ✅ 40 passed (run id 32308722) |
| `cargo test -p oxidean-db --test dialect_packages` | ✅ ok |
| Key files (oci/npm/generic/acl/rpc/quota/gc, dialect_packages, smoke-packages) | ✅ present |

**Verdict:** `status: validated`, `nyquist_compliant: true`. All 14 task-map rows green. Residual: live `docker`/`npm` client paths and type-to-confirm UX (manual); OCI referrers deferred per `20-VERIFICATION.md` caveats (non-blocking for Nyquist sampling of shipped PKG-01..05).

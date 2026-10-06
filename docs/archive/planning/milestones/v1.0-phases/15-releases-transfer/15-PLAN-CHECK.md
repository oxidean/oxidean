# Phase 15 Plan Check — Releases & Transfer

**Checked:** 2026-09-14  
**Branch:** `feat/forge-core`  
**Plans verified:** 6 (`15-00` … `15-05`)  
**Phase goal:** Users can ship tagged releases with assets and rename or transfer repositories they administer  
**Requirements:** GIT-14, GIT-15, GIT-16, GIT-17  

## CHECK FAILED

**Issues:** 3 blocker class(es) (15× missing `<fails_when>` + research open questions + plan-01 scope), 3 warning(s), 2 info  

Plans must be revised before `/gsd-execute-phase 15`.

---

### Coverage Summary

| Requirement | Plans | Status |
|-------------|-------|--------|
| GIT-14 | 00, 01, 02, 05 | Covered (notes 01 → assets 02 → UI/ops 05) |
| GIT-15 | 00, 02, 05 | Covered |
| GIT-16 | 00, 03, 05 | Covered |
| GIT-17 | 00, 04, 05 | Covered |

### Decision Coverage (CONTEXT.md)

| Decision | Status |
|----------|--------|
| D-REL-01 … D-REL-13 | Referenced in plan actions/assumptions/prohibitions |
| Deferred ideas | Excluded (no auto-changelog, soft-delete recycle, cross-instance, Phase-20 packages) |

### Plan Summary

| Plan | Tasks | files_modified | Wave | depends_on | Estimate | Structure |
|------|-------|----------------|------|------------|----------|-----------|
| 00 | 2 | 5 | 0 | [] | 22k (ok) | Valid |
| 01 | 3 | **18** | 1 | [00] | 72k (~72%) | Valid |
| 02 | 2 | 12 | 2 | [01] | 64k | Valid |
| 03 | 3 | 13 | 3 | [01,02] | 70k | Valid |
| 04 | 2 | 7 | 4 | [03] | 56k | Valid |
| 05 | 3 | 8 | 5 | [02,04] | 52k | Valid |

Dependency graph: acyclic; waves consistent (`wave = max(deps)+1`). No same-wave plan pairs → Dimension 3b N/A.

---

## Dimension Results

| # | Dimension | Result |
|---|-----------|--------|
| 1 | Requirement coverage | PASS |
| 2 | Task completeness | PASS (files/action/verify/done present) |
| 3 | Dependency correctness | PASS |
| 3b | Undeclared coupling | PASS (no same-wave pairs) |
| 4 | Key links planned | WARNING — web HTTP redirect path thin |
| 5 | Scope sanity | **FAIL** — 15-01 18 files; 02/03 ≥10 files |
| 6 | Verification derivation | PASS (user-observable truths) |
| 7 | Context compliance | PASS (locked decisions; deferred excluded) |
| 7b | Scope reduction | PASS (cross-plan split, not decision shrink) |
| 7c | Architectural tier | WARNING — web redirect primary tier under-tasked |
| 8 | Nyquist compliance | **FAIL** — 8f missing `<fails_when>` on all 15 runnable verifies |
| 9 | Cross-plan data contracts | PASS |
| 10 | `.cursor/rules/` compliance | PASS (`make rpc-gen`, Octane `.tsrx`, dialect in db, no new pkgs) |
| 11 | Research resolution | **FAIL** — `## Open Questions` not `(RESOLVED)` |
| 12 | Pattern compliance | SKIPPED (no PATTERNS.md) |
| — | Verify path probe | Silent / not_applicable (no `cd`/`npm --prefix` forms) |
| — | Failing-direction probe | **blocked** — 15 commands missing `<fails_when>` |

### Dimension 8: Nyquist Compliance

| Task | Plan | Wave | Automated | Failing Direction | Status |
|------|------|------|-----------|-------------------|--------|
| 15-00-T1 | 00 | 0 | nextest list + file exists | ❌ missing | ❌ |
| 15-00-T2 | 00 | 0 | `test -f` web stubs | ❌ missing | ❌ |
| 15-01-T1 | 01 | 1 | rpc-gen + dialect + release_create/tag | ❌ missing | ❌ |
| 15-01-T2 | 01 | 1 | release_draft/update/delete | ❌ missing | ❌ |
| 15-01-T3 | 01 | 1 | Vitest releases integration | ❌ missing | ❌ |
| 15-02-T1 | 02 | 2 | release_asset nextest | ❌ missing | ❌ |
| 15-02-T2 | 02 | 2 | rg compose/docs/vite + build | ❌ missing | ❌ |
| 15-03-T1 | 03 | 3 | repo_rename nextest | ❌ missing | ❌ |
| 15-03-T2 | 03 | 3 | redirect/rename/git_smart | ❌ missing | ❌ |
| 15-03-T3 | 03 | 3 | retention rg + redirect nextest | ❌ missing | ❌ |
| 15-04-T1 | 04 | 4 | repo_transfer nextest | ❌ missing | ❌ |
| 15-04-T2 | 04 | 4 | transfer/cascade nextest | ❌ missing | ❌ |
| 15-05-T1 | 05 | 5 | Vitest settings+releases | ❌ missing | ❌ |
| 15-05-T2 | 05 | 5 | factory_reset/release_asset | ❌ missing | ❌ |
| 15-05-T3 | 05 | 5 | docs rg + rpc-sync-check | ❌ missing | ❌ |

Sampling: each wave’s tasks have automated verifies → 8c OK.  
Wave 0 stub paths match VALIDATION.md → 8d OK.  
VALIDATION.md present → 8e OK.  
Failing directions: **0/15** → 8f ❌  
Overall Nyquist: ❌ FAIL

---

## Blockers

**1. [nyquist_compliance / 8f] Every runnable `<automated>` has a stated `<fails_when>`**
- Plan: all (`15-00` … `15-05`)
- Evidence: `gsd_run check verify-failure-directions 15` → `status: blocked`, `counts.blocker: 15`. Grep finds zero `<fails_when>` in the phase dir.
- Example fix (non-binding): Add a `<fails_when>` sibling after each `<automated>` naming an observable failure signal (non-zero exit and/or missing expected output).

**2. [research_resolution] RESEARCH.md carries no unresolved open question**
- Plan: phase-level (`15-RESEARCH.md`)
- Evidence: Heading is `## Open Questions` (no `(RESOLVED)`). Four items (migration number, rename confirm, SSH redirect scope, latest release) lack inline `RESOLVED` markers. Plans already lock answers in assumptions (next-free `00NN`, simple rename form, SSH via shared helper, no `release.latest`), but research artifact is still open.
- Example fix (non-binding): Mark each question RESOLVED (or retitle `## Open Questions (RESOLVED)`) to match plan locks.

**3. [scope_sanity] Each plan stays within the per-plan context budget (files ≤14)**
- Plan: `15-01`
- Evidence: `files_modified` lists **18** paths (blocker threshold 15+). Task 1 alone lists **16** comma-separated `<files>` (migration + RPC + UI tracer in one task).
- Example fix (non-binding): Split tracer (schema+RPC) from UI/chrome into a follow-on plan, or move detail/update UI into existing Task 2/3 only and shrink Task 1’s file set.

---

## Warnings

**1. [scope_sanity] Plans stay near file-count warning band**
- Plans: `15-02` (12 files), `15-03` (13 files)
- Evidence: Warning threshold is 10 files/plan; both exceed it while staying under blocker.
- Example fix (non-binding): Peel docs/ENV or purge-job tasks into a thinner trailing plan if execution context pressure appears.

**2. [key_links_planned / architectural_tier_compliance] D-REL-08 web path redirect is wired in API/git, not clearly in Frontend Server**
- Plan: `15-03` (truth claims old `/{owner}/{repo}` redirects); `15-05` only *assumes* loaders may navigate
- Evidence: Architectural Responsibility Map assigns web redirect resolution to Frontend Server (SSR). Plan 03 tasks touch `rename_transfer.rs`, Smart HTTP, SSH — no `apps/web` loader/`throw redirect` task. Plan 05 assumption is soft (“may call”).
- Example fix (non-binding): Add an explicit web loader/SSR task (or Axum HTML 302 middleware) with files under `apps/web` (or documented API middleware) so old browser paths redirect within retention.

**3. [scope_sanity] Plan 01 estimate sits at ~70% smart-zone**
- Plan: `15-01`
- Evidence: `estimate-check --calibrated` → 72000 / 100000 (`ratio: 0.72`), `confidence: low`, `over_budget: false`.
- Example fix (non-binding): Same split as blocker 3 reduces token risk; treat figure as uncalibrated (sample_count 0).

---

## Advisories (info)

**1. [dependency_correctness] Ordering between plans is declared, not implied**
- Plans: `15-03` → depends_on `02`
- Evidence: Rename/redirect does not consume release-asset HTTP; depending on `02` serializes unnecessarily after assets.
- Example fix (non-binding): `depends_on: ["01"]` only (keep wave ≥2), or leave as intentional serialization.

**2. [scope_sanity] Estimate confidence is low across all plans**
- Evidence: All plans `confidence: low`; calibration sample_count 0 — weigh file/task thresholds more than token figures.

---

## Structured Issues

```yaml
issues:
  - plan: null
    dimension: nyquist_compliance
    severity: blocker
    required_property: "Every runnable <automated> verify has a non-empty <fails_when> sibling"
    description: "All 15 automated verifies across 15-00..15-05 lack <fails_when>; verify-failure-directions probe status=blocked, blocker=15"
    fix_hint: "Add <fails_when> after each <automated> naming exit/output failure signal"

  - plan: null
    dimension: research_resolution
    severity: blocker
    required_property: "RESEARCH.md carries no unresolved open question"
    description: "15-RESEARCH.md ## Open Questions has no (RESOLVED) suffix; migration number, rename confirm, SSH scope, and latest release lack RESOLVED markers"
    file: "15-RESEARCH.md"
    unresolved_questions:
      - "Exact migration number"
      - "Rename confirmation"
      - "SSH redirect scope"
      - "Latest release semantics"
    fix_hint: "Resolve to match plan assumptions and mark ## Open Questions (RESOLVED)"

  - plan: "15-01"
    dimension: scope_sanity
    severity: blocker
    required_property: "Each plan stays within the per-plan file budget (<15 files_modified)"
    description: "Plan 01 files_modified has 18 entries; Task 1 lists 16 files spanning migration, RPC, rpc-gen, and Releases UI"
    task: 1
    metrics:
      tasks: 3
      files: 18
    fix_hint: "Split schema/RPC tracer from Octane Releases UI into separate plans or shrink Task 1 files"

  - plan: "15-02"
    dimension: scope_sanity
    severity: warning
    required_property: "Each plan stays within the per-plan context budget"
    description: "Plan 02 has 12 files_modified (warning band ≥10)"
    metrics:
      tasks: 2
      files: 12
    fix_hint: "Optionally move Compose/docs/vite into a docs-ops micro-plan"

  - plan: "15-03"
    dimension: scope_sanity
    severity: warning
    required_property: "Each plan stays within the per-plan context budget"
    description: "Plan 03 has 13 files_modified (warning band ≥10)"
    metrics:
      tasks: 3
      files: 13
    fix_hint: "Optionally separate purge-job/docs from rename+resolve core"

  - plan: "15-03"
    dimension: key_links_planned
    severity: warning
    required_property: "Dependent artifacts for D-REL-08 web redirects are wired by a task, not merely assumed"
    description: "must_haves claim old /{owner}/{repo} redirects, but no apps/web loader/SSR task; 15-05 only assumes loaders may resolve"
    fix_hint: "Add explicit web redirect navigation task or document Axum HTML 302 as the web surface"

  - plan: "15-03"
    dimension: architectural_tier_compliance
    severity: warning
    required_property: "Each capability sits in its Responsibility Map tier"
    description: "Web redirect resolution is Frontend Server (SSR) in RESEARCH map; plan tasks only API/Smart HTTP/SSH"
    capability: "Redirect resolution (web)"
    expected_tier: "Frontend Server (SSR)"
    actual_tier: "API / Backend (+ git)"
    fix_hint: "Task SSR/loader redirect in apps/web or justify API-tier HTTP 302 as primary web surface"

  - plan: "15-01"
    dimension: scope_sanity
    severity: warning
    required_property: "Each plan stays within the per-plan context budget"
    description: "estimate.tokens 72000 is ~72% of 100000 smart zone; confidence low / uncalibrated"
    fix_hint: "Re-slice with blocker-3 split to lower token pressure"

  - plan: "15-03"
    dimension: dependency_correctness
    severity: info
    required_property: "Ordering between plans is declared, not implied"
    description: "Plan 03 depends_on 02 though rename/redirect does not need release assets"
    plans: ["02", "03"]
    fix_hint: "depends_on: ['01'] only, or mark intentional serialization"

  - plan: null
    dimension: scope_sanity
    severity: info
    required_property: "Estimate confidence is interpreted correctly when sample_count is low"
    description: "All six plans report confidence: low with calibration sample_count 0"
    fix_hint: "Prefer file/task thresholds over token estimates until actuals exist"
```

---

## What already looks solid

- Requirement IDs claimed in frontmatter; goal-backward path covers notes → assets → rename → transfer → settings/ops.
- Locked D-REL decisions mapped; deferred ideas not in scope; `make rpc-gen` / Octane / dialect-in-db honored.
- Wave 0 stubs align with `15-VALIDATION.md`; TDD filters named consistently.
- Threat models present; no new packages; asset volume ≠ LFS; type-confirm transfer; Admin/Write gates explicit.
- Cross-plan schema handoff (`releases`/`release_assets`/`repository_redirects` in 01 → consume in 02–04) is compatible.

---

## Recommendation

**3 blocker classes + warnings require planner revision** before execution:

1. Add `<fails_when>` to every `<automated>` verify (15 sites).
2. Close `15-RESEARCH.md` Open Questions as RESOLVED to match plan locks.
3. Re-slice `15-01` below 15 `files_modified` (and shrink Task 1).
4. (Recommended) Explicit web redirect task for D-REL-08; optionally narrow `15-03`’s `depends_on`.

Returning to planner with feedback.

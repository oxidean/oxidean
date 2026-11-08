---
phase: "11"
slug: "issues"
checker: gsd-plan-checker
checked: "2026-09-14"
status: failed
plans_checked: 13
blocker_count: 1
warning_count: 5
info_count: 2
---

# Phase 11 — Plan Check

## CHECK FAILED

**Phase:** 11-issues — Users can track work with issues, comments, labels, assignees, and links to PRs  
**Plans checked:** 13 (`11-00` … `11-12`)  
**Issues:** 1 blocker(s), 5 warning(s), 2 info

Revision required before `/gsd-execute-phase 11`.

---

### Coverage Summary

| Requirement | Plans (claim) | Status |
|-------------|---------------|--------|
| ISS-01 | 00, 01, 02, 03, 04, 08, 11, 12 | Covered (create/edit/close/reopen + list IA + ACL) |
| ISS-02 | 00, 01, 02, 05, 08, 12 | Covered (comments + moderation + history) |
| ISS-03 | 00, 01, 02, 06, 07, 11, 12 | Covered (labels + multi-assignees) |
| ISS-04 | 00, 01, 02, 09, 10, 12 | Covered (autolink + link stubs + manual link) |

| Locked decision | Implementing plans | Status |
|-----------------|-------------------|--------|
| D-ISS-01…04 | 00, 02, 03, 04 | Covered |
| D-ISS-05…08 | 00, 01, 06, 07 | Covered |
| D-ISS-09…12 | 00, 01, 05, 08, 10 | Covered |
| D-ISS-13…15 | 00, 01, 03, 09, 10, 12 | Covered (15 deferred correctly) |
| D-ISS-16…19 | 01, 03, 11 | Covered |
| D-ISS-20 | 00, 01, 03+ | Covered |

Deferred Ideas (closing keywords, full PR objects, notifications, GitHub search grammar, milestones/projects) are excluded or explicitly prohibited — no scope creep.

---

### Plan Summary

| Plan | Tasks | Files | Wave | depends_on | Structure |
|------|-------|-------|------|------------|-----------|
| 00 | 2 | 10 | 0 | [] | Valid |
| 01 | 2 | 2 | 0 | [] | Valid |
| 02 | 2 (1 checkpoint) | 9 | 1 | 00,01 | Valid |
| 03 | 3 | 13 | 2 | 02 | Valid |
| 04 | 3 | 8 | 3 | 03 | Valid |
| 05 | 2 | 9 | 4 | 04 | Valid |
| 06 | 3 | 11 | 5 | 05 | Valid |
| 07 | 2 | 7 | 6 | 06 | Valid |
| 08 | 2 | 8 | 7 | 07 | Valid |
| 09 | 2 | 7 | 8 | 08 | Valid |
| 10 | 2 | 5 | 5 | 05 | Valid |
| 11 | 2 | 7 | 9 | 07,09 | Valid |
| 12 | 2 | 7 | 10 | 08,10,11 | Valid |

Dependency graph: acyclic; wave numbers consistent with deps; same-wave pairs `00|01` and `06|10` have no `files_modified` overlap.

---

### Dimension results

| Dim | Result | Notes |
|-----|--------|-------|
| 1 Requirement coverage | PASS | All ISS-01..04 claimed + tasked |
| 2 Task completeness | PASS* | Structure valid; UI verifies weak (see warnings) |
| 3 Dependency correctness | PASS | Acyclic; waves OK |
| 3b Undeclared coupling | PASS | No same-wave shared mutable writer |
| 4 Key links planned | PASS | RPC↔UI↔DB wiring described |
| 5 Scope sanity | WARN | Plans 00/03/06 over file warning threshold |
| 6 must_haves derivation | PASS | User-observable truths |
| 7 Context compliance | PASS | D-ISS-* covered; deferred excluded |
| 7b Scope reduction | PASS | Linked PR “stubs” match D-ISS-13 (not silent reduction) |
| 7c Architectural tiers | PASS | Matches RESEARCH Responsibility Map |
| 8 Nyquist | WARN* | VALIDATION present; Wave 0 stubs map complete; see UI verify + 8f note |
| 9 Cross-plan data contracts | PASS | No conflicting transforms |
| 10 .cursor/rules | PASS | Octane `.tsrx`, `make rpc-gen`, dialect SQL in db |
| 11 Research resolution | **FAIL** | Open Questions not marked RESOLVED |
| 12 Pattern compliance | SKIPPED | No PATTERNS.md |
| Verify path / format | WARN | Wave 0 `\|\| true`; build-only UI tasks |

\*Dimension 8f (`fails_when` probe) was not supplied in the checker prompt — not scored as deterministic 8f blockers. Plans currently have **0** `<fails_when>` siblings across 29 `<automated>` commands (advisory).

---

### Dimension 8: Nyquist Compliance (summary)

| Area | Status |
|------|--------|
| VALIDATION.md exists | ✅ |
| Wave 0 Rust stubs (00) | ✅ maps VALIDATION gaps |
| Wave 0 Vitest stubs (01) | ✅ Issues UI + markdown.issues |
| Per-task `<automated>` | ✅ (no `MISSING` tokens) |
| Sampling continuity | ✅ |
| Failing directions (`fails_when`) | ⏸ probe absent / not stated in plans |
| Overall | ❌ FAIL (with research + verify warnings) |

---

### Blockers — must hold before execution

**1. [research_resolution] RESEARCH.md carries no unresolved open question**
- Plan: null (phase-level)
- Evidence: `11-RESEARCH.md` has `## Open Questions` without `(RESOLVED)` suffix; Q1–Q3 lack inline `RESOLVED` markers (only “Recommendation:”). Plans already encode answers (11-10 disables mention/commit autolink; 11-06 places org+repo Labels settings; soft not-found for private targets), but research is not formally closed.
- Example fix (non-binding): Rename to `## Open Questions (RESOLVED)` and mark each item `RESOLVED:` with the plan-locked answer.

---

### Warnings — should hold

**1. [scope_sanity] Each plan stays within the per-plan context budget**
- Plan: 11-03
- Evidence: `files_modified` count **13** (warning threshold 10; blocker 15+). Tracer spans API module + rpc + client + chrome + three routes + list + integration test.
- Example fix: Split UI routes (T2/T3) into a dependent plan, or trim files owned by later plans from frontmatter until touched.

**2. [scope_sanity] Each plan stays within the per-plan context budget**
- Plan: 11-06
- Evidence: `files_modified` count **11** (labels API + db + org/repo settings routes + detail + integration).
- Example fix: Move org settings Labels route to its own plan or fold defs-only vs assign UI.

**3. [scope_sanity] Each plan stays within the per-plan context budget**
- Plan: 11-00
- Evidence: `files_modified` count **10** (all Wave 0 Rust stubs in one plan — at warning threshold).
- Example fix: Acceptable to keep Wave 0 together; optional split API stubs vs dialect/factory_reset.

**4. [nyquist_compliance] UI task completion is decided by an automated check that can fail on behavior**
- Plan: 11-04 task 3; 11-08 task 2
- Evidence: Both UI tasks verify only with `bun --cwd apps/web run build`. Lifecycle/history/delete UI (D-ISS-02/04) and reaction bar UI (D-ISS-11) have no Vitest/integration assertion greened in those plans, despite Wave 0 Issues integration stubs existing.
- Example fix: Extend `$owner.$repo.issues.integration.test.ts` cases and run them in those task `<verify>` blocks (or explicitly defer UI assertions to a later plan’s green-up with a named stub list).

**5. [verify_command_format] Wave 0 verify must not swallow runner failure without a separate falsifiable signal**
- Plan: 11-01 tasks 1–2
- Evidence: `<automated>` uses `vitest run … || true; test -f …`. A syntax-broken stub file still passes via `test -f` alone.
- Example fix: Prefer discoverability checks that fail closed (e.g. vitest list/parse without requiring green assertions), or assert file content tokens without `|| true` masking parse errors.

---

### Advisories (info)

**1. [nyquist_compliance] Stated failing direction siblings**
- Plan: all
- Evidence: No `<fails_when>` elements on 29 runnable `<automated>` commands; Dimension 8f probe was not injected into this checker run.
- Example fix: Add `<fails_when>` naming exit/output failure signals next to each `<automated>`.

**2. [scope_sanity] Estimate calibration confidence is low**
- Plan: all
- Evidence: Every plan `estimate.confidence: low` / `sample_count: 0`; token estimates within smart-zone budget (max ~52k vs 100k) but uncalibrated.
- Example fix: No revision required; weigh file/task thresholds more than token estimates.

---

### Structured Issues

```yaml
issues:
  - plan: null
    dimension: research_resolution
    severity: blocker
    required_property: "RESEARCH.md carries no unresolved open question"
    description: "11-RESEARCH.md ## Open Questions lacks (RESOLVED) suffix; Q1 mention autolink, Q2 org Labels nav, Q3 cross-repo soft-404 have Recommendation only, no RESOLVED markers"
    fix_hint: "Mark section ## Open Questions (RESOLVED) and tag each question RESOLVED with the answer already locked in plans 06/10/ACL"

  - plan: "11-03"
    dimension: scope_sanity
    severity: warning
    required_property: "Each plan stays within the per-plan context budget"
    description: "Plan 03 lists 13 files_modified (warning ≥10)"
    fix_hint: "Split tracer UI routes into a follow-on plan after RPC green"

  - plan: "11-06"
    dimension: scope_sanity
    severity: warning
    required_property: "Each plan stays within the per-plan context budget"
    description: "Plan 06 lists 11 files_modified (warning ≥10)"
    fix_hint: "Split org Labels settings from repo assign UI / issue.labels.set"

  - plan: "11-00"
    dimension: scope_sanity
    severity: warning
    required_property: "Each plan stays within the per-plan context budget"
    description: "Plan 00 lists 10 files_modified (at warning threshold)"
    fix_hint: "Optional: split dialect/factory_reset stubs from issue_* API stubs"

  - plan: "11-04"
    task: 3
    dimension: nyquist_compliance
    severity: warning
    required_property: "UI task completion is decided by an automated check that can fail on behavior"
    description: "11-04-T3 verify is only bun build; edit/close/reopen/history/delete UI has no integration greening"
    fix_hint: "Green matching issues.integration.test.ts cases in T3 verify"

  - plan: "11-08"
    task: 2
    dimension: nyquist_compliance
    severity: warning
    required_property: "UI task completion is decided by an automated check that can fail on behavior"
    description: "11-08-T2 verify is only bun build; reaction bar has no automated UI assertion"
    fix_hint: "Add/green integration stub for reaction bar visibility + Write+ toggle affordance"

  - plan: "11-01"
    dimension: verify_command_format
    severity: warning
    required_property: "Wave 0 verify must not swallow runner failure without a separate falsifiable signal"
    description: "Vitest invoked with || true then test -f; parse/load failures still pass"
    fix_hint: "Replace || true with a discoverability command that fails on unparseable stubs"

  - plan: null
    dimension: nyquist_compliance
    severity: info
    required_property: "Runnable automated verifies state a failing direction"
    description: "0/29 automated commands have <fails_when>; 8f probe not supplied to checker"
    fix_hint: "Add <fails_when> siblings (e.g. non-zero exit / missing filter match)"

  - plan: null
    dimension: scope_sanity
    severity: info
    required_property: "Estimate confidence is reported honestly"
    description: "All plans estimate.confidence=low with no calibration samples; all under 100k token budget"
    fix_hint: "No action required for execute readiness"
```

---

### Recommendation

1 blocker + 5 warnings require planner revision (or explicit accept of scope/verify warnings after fixing research resolution).

Minimal path to re-check:
1. Resolve `11-RESEARCH.md` Open Questions formally.
2. Tighten 11-04-T3 / 11-08-T2 verifies (and optionally 11-01 Wave 0 verify).
3. Optionally re-slice 11-03 / 11-06 file load.

---

### Revision follow-up (2026-09-14)

Planner revision after this check (`docs(11): revise plans from check`):

| Issue | Disposition |
|-------|-------------|
| Blocker research_resolution | Fixed — `11-RESEARCH.md` `## Open Questions (RESOLVED)` with Q1–Q3 `RESOLVED:` locks (11-10 / 11-06 / 11-03+ACL) |
| Warning verify_command_format (11-01) | Fixed — Wave 0 uses `test -f && vitest list` (no `\|\| true`) |
| Warning nyquist UI verify (11-04-T3, 11-08-T2) | Fixed — build + green `issues.integration.test.ts` cases |
| Warning scope_sanity 11-00 (10 files) | Accepted — Wave 0 Rust stubs stay one plan; split cost outweighs benefit |
| Warning scope_sanity 11-03 (13 files) | Accepted — tracer vertical slice kept intact; splitting UI routes would explode plan count |
| Warning scope_sanity 11-06 (11 files) | Accepted — org+repo Labels + assign UI stay together per D-ISS-05; split deferred |
| Info fails_when / estimate confidence | No action — advisory only |

Re-run plan-checker after this revision.

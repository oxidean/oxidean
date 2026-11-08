---
phase: 20-packages-registry
gate: revision
checker: gsd-plan-checker
checked: 2026-09-14
plans_checked: 13
status: issues_found
blockers: 4
warnings: 2
infos: 2
---

# Phase 20 — Plan Check

## CHECK FAILED

**Phase:** Packages Registry  
**Plans verified:** 13 (`20-00` … `20-12`)  
**Issues:** 4 blocker(s), 2 warning(s), 2 info  
**Focus:** Coverage gaps and blockers (large phase; nits de-emphasized)

### Coverage Summary

| Requirement | Plans (frontmatter) | Status |
|-------------|---------------------|--------|
| PKG-01 | 00, 02, 05, 09, 12 (+01 smoke) | Covered — OCI push/pull/tags/delete + Bearer (05), quotas (09) |
| PKG-02 | 00, 02, 06, 07, 09, 12 (+01) | Covered — publish/install (06) + dist-tags/deprecate/search (07) |
| PKG-03 | 00, 02, 04, 09, 12 (+01) | Covered — tracer generic PUT/GET/DELETE/list (04) |
| PKG-04 | 00, 02, 03, 04, 05, 06, 11, 12 | Covered — hybrid ACL∩PAT, anon public pull, edge routing |
| PKG-05 | 00, 01, 08, 10, 11, 12 | Covered — list/delete RPC + owner/repo UI + type-to-confirm |

| Locked decision | Delivering plans | Status |
|-----------------|------------------|--------|
| D-PKG-01…03 routing/namespace | 02, 04, 05, 06, 12 | Covered |
| D-PKG-04…06 hybrid auth / ACL | 03, 05, 06, 08, 11 | Covered |
| D-PKG-07…09 store / quotas / GC | 02, 03, 09, 11 | Covered |
| D-PKG-10…12 immutability / UI / confirm | 04–08, 10 | Covered |
| D-PKG-13…16 full OCI+npm+generic | 04–07 | Covered (split across plans, not reduced) |

Deferred ideas (Maven/PyPI/S3/cosign/replication): excluded — cosign/referrers explicitly deferred under Claude discretion.

### Plan Summary

| Plan | Tasks | Wave | depends_on | Status |
|------|-------|------|------------|--------|
| 00 | 2 | 0 | [] | Valid structure |
| 01 | 2 | 0 | [] | Valid structure |
| 02 | 3 | 1 | 00 | **T2 verify/action mismatch** |
| 03 | 3 | 2 | 02 | **T1 verify swallows failure** |
| 04 | 2 | 3 | 03 | Valid (tracer) |
| 05 | 3 | 4 | 04 | Valid |
| 06 | 2 | 4 | 04 | Valid |
| 07 | 2 | 5 | 06 | Valid |
| 08 | 2 | 5 | 04,05,06 | Valid |
| 09 | 3 | 6 | 04,05,06 | **Missing depends_on 08** |
| 10 | 2 | 6 | 01,08 | Valid |
| 11 | 2 | 7 | 03,09 | Valid |
| 12 | 2 | 8 | 05–11 | Gate verify incomplete (warn) |

---

### Blockers — these properties must hold

**1. [research_resolution] RESEARCH.md carries no unresolved open question**
- Plan: null (phase-level)
- Evidence: `20-RESEARCH.md` has `## Open Questions` without `(RESOLVED)` and no inline `RESOLVED` markers on the three questions (migration id collision; classic `repo` ⇒ packages; OCI nested names). Plans already adopt the recommendations (00xx migration resolve-at-execute; fail-closed package scopes; nested OCI allowed) but Dimension 11 still requires an explicit resolution record.
- Example fix (non-binding): Rewrite as `## Open Questions (RESOLVED)` and mark each item `RESOLVED:` with the adopted answer.

**2. [dependency_correctness] Cross-plan artifact producers are declared in `depends_on`**
- Plan: `20-09`
- Evidence: Plan 09 `files_modified` and Task 2 edit `crates/oxidean-api/src/packages/rpc.rs` and run `package_rpc` / `make rpc-gen`, but that module and list/delete RPCs are introduced in plan **08**. `depends_on` is only `["04","05","06"]` — no edge to `08`. Wave 6 ordering may hide this in a full-phase run; isolated or reordered execution breaks Admin quota RPC on a missing foundation.
- Example fix (non-binding): Add `"08"` to plan 09 `depends_on` (wave stays ≥6).

**3. [verify_command_format] Automated verifies must not swallow failures into a passing fallback**
- Plan: `20-03`
- Task: 1 (`20-03-T1`)
- Evidence: Verify is  
  `cargo nextest … ; cargo test -p oxidean-db --lib -- packages 2>/dev/null || cargo check -p oxidean-api -p oxidean-db`  
  If nextest/db tests fail, `|| cargo check` can still exit 0 and mark the store task done.
- Example fix (non-binding): Drop the `|| cargo check` fallback; require nextest/db tests (or a single non-masking command) to be the pass/fail signal.

**4. [task_completeness] Task action, verify, and done must agree on the same deliverable**
- Plan: `20-02`
- Task: 2 (`20-02-T2`)
- Evidence: `<action>` only adds `OXIDEAN_PACKAGES_DIR` / volume / docs and forbids quota enforcement. `<verify>` and `<done>` also require Traefik `PathPrefix` / `api-packages` labels that Task 3’s action is what actually adds. T2 cannot pass verify from its own action.
- Example fix (non-binding): Move PathPrefix/`api-packages` assertions to T3 only, or expand T2 action to own those Compose labels.

---

### Warnings — these properties should hold

**1. [verification_derivation] Phase-gate verify matches the gate behaviors the task claims**
- Plan: `20-12`
- Task: 2
- Evidence: Action requires “web packages Vitest” in the phase gate; `<automated>` runs nextest + dialect_packages + rpc-sync-check + smoke only — no Vitest. PKG-05 UI is greened in 10/11, but the phase gate as written under-verifies UI.
- Example fix (non-binding): Add Vitest filters for packages routes to the gate command, or narrow the action to match the automated gate.

**2. [nyquist_compliance] VALIDATION.md remains draft (`nyquist_compliant: false`)**
- Plan: null
- Evidence: Expected pre-`/gsd-validate-phase`; Wave 0 stubs are planned (00/01) and map to VALIDATION checklist. Not an execution blocker if Wave 0 lands first, but Nyquist sign-off is still open.
- Example fix (non-binding): After Wave 0 + sampling continuity, run `/gsd-validate-phase` to flip the flag.

---

### Advisories (info)

**1. [dependency_correctness] Ordering between same-wave plans is declared, not implied**
- Plans: `05`, `06` (both Wave 4, no mutual `depends_on`)
- Evidence: Both expand mounts created in 04 (`oci.rs` / `npm.rs`). No `files_modified` overlap; actions say do not remount. Safe if 04 left module wiring complete; otherwise declare `coupling_justified` or serialize.
- Example fix (non-binding): No change if 04 owns all `mod.rs`/router wiring; else add an explicit edge.

**2. [scope_sanity] Estimate confidence is `low` (sample_count 0)**
- Plan: all with `estimate`
- Evidence: Per-plan estimates 18k–56k tokens, under 100k budget (`over_budget: false`). Calibration not yet meaningful for this project.
- Example fix (non-binding): None required for acceptance; weigh task/file counts (all ≤3 tasks) more than token figures.

---

### Dimension notes (abbreviated)

| Dim | Result |
|-----|--------|
| 1 Requirement coverage | PASS — PKG-01…05 claimed and tasked |
| 2 Task completeness | FAIL — 02-T2 action/verify mismatch |
| 3 Dependencies | FAIL — 09→08 undeclared; graph otherwise acyclic |
| 3b Undeclared coupling | INFO — 05/06 wave-4 advisory |
| 4 Key links | PASS — store↔ACL↔protocols↔RPC↔UI wired in must_haves/actions |
| 5 Scope sanity | PASS — 2–3 tasks/plan; estimates in budget |
| 6 must_haves | PASS — user-observable truths |
| 7 Context compliance | PASS — D-PKG-* honored; deferred ecosystems excluded; no silent scope reduction of D-PKG-13/14 |
| 7b Scope reduction | PASS — npm features split 06→07 is plan slicing, not decision shrink |
| 7c Architectural tiers | PASS — protocols/ACL/store/UI/Traefik match RESEARCH map |
| 8 Nyquist | PARTIAL — VALIDATION exists; automated verifies present; Wave 0 planned; `fails_when` probe not supplied (8f silent); flag still false (warn) |
| 9 Data contracts | PASS — no conflicting transforms on blob/metadata path |
| 10 .cursor/rules | PASS — Octane `.tsrx`, `make rpc-gen`, dialect SQL in db, no hand-edit-as-SoT |
| 11 Research resolution | FAIL — Open Questions unmarked |
| 12 Pattern compliance | SKIPPED (no PATTERNS.md) |

---

### Structured Issues

```yaml
issues:
  - plan: null
    dimension: research_resolution
    severity: blocker
    required_property: "RESEARCH.md carries no unresolved open question"
    description: "20-RESEARCH.md ## Open Questions lacks (RESOLVED) suffix and inline RESOLVED markers on migration id, classic repo⇒packages, and OCI nested-name questions"
    fix_hint: "Mark section ## Open Questions (RESOLVED) with adopted answers already reflected in plans"

  - plan: "20-09"
    dimension: dependency_correctness
    severity: blocker
    required_property: "Cross-plan artifact producers are declared in depends_on"
    description: "Plan 09 edits packages/rpc.rs and package_rpc tests introduced by plan 08 but depends_on only lists 04,05,06"
    fix_hint: "Add depends_on: [\"08\"] (keep wave ≥6)"

  - plan: "20-03"
    task: 1
    dimension: verify_command_format
    severity: blocker
    required_property: "Automated verifies must not swallow failures into a passing fallback"
    description: "20-03-T1 verify uses `2>/dev/null || cargo check` after test commands, so failing tests can still yield exit 0"
    fix_hint: "Remove cargo check fallback; let nextest/db test exit codes decide"

  - plan: "20-02"
    task: 2
    dimension: task_completeness
    severity: blocker
    required_property: "Task action, verify, and done agree on the same deliverable"
    description: "20-02-T2 action only covers PACKAGES_DIR/volume/docs but verify/done require Traefik PathPrefix/api-packages owned by T3"
    fix_hint: "Scope T2 verify to env/volume only; assert PathPrefix in T3"

  - plan: "20-12"
    task: 2
    dimension: verification_derivation
    severity: warning
    required_property: "Phase-gate verify matches the gate behaviors the task claims"
    description: "20-12-T2 action includes web packages Vitest in the phase gate but automated verify omits Vitest"
    fix_hint: "Add packages Vitest to gate command or drop Vitest from the action gate list"

  - plan: null
    dimension: nyquist_compliance
    severity: warning
    required_property: "VALIDATION.md Nyquist sign-off is completed before verify-work"
    description: "20-VALIDATION.md has nyquist_compliant: false and wave_0_complete: false (expected until validate-phase after Wave 0)"
    fix_hint: "Land Wave 0 then /gsd-validate-phase"

  - plan: "20-05"
    dimension: dependency_correctness
    severity: info
    required_property: "Ordering between same-wave plans is declared, not implied"
    description: "Plans 05 and 06 are both Wave 4 with no mutual depends_on; both expand 04 mounts without files_modified overlap"
    plans: ["05", "06"]
    fix_hint: "Declare coupling_justified or leave as-is if 04 completed router wiring"

  - plan: null
    dimension: scope_sanity
    severity: info
    required_property: "Each plan stays within the per-plan context budget"
    description: "All plan estimates under budget but confidence:low (no calibration samples); task counts 2–3 are within target"
    fix_hint: "No split required on task/file thresholds"
```

---

### Recommendation

4 blocker(s), 2 warning(s) require revision. Returning to planner with feedback.

**Not blocking coverage:** PKG-01…05 and D-PKG-01…16 are planned end-to-end (OCI/npm/generic + hybrid auth + quotas/GC + UI/docs). Fix the four blockers above, then re-run plan-check.

## CHECK FAILED

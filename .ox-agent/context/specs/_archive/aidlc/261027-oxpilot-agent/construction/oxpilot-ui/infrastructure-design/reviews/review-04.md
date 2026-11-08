### Architecture Review — oxpilot-ui infrastructure-design (iteration 4)

### Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY

**Iteration:** 4

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Major | `cicd-pipeline.md` + `infrastructure-design-questions.md` Q3; iter-2/3 residuals at `monitoring-design.md` + `traceability.json` NFR7.1 | Browser-coverage manifest plan pinned rows on files the gate's `HIGH_RISK_RE` doesn't discover → orphan-fail (`browser-coverage-check.ts:255-259`) | Pin browser rows only on discovered surfaces; rebind `oxpilot-repo-trigger`; ride kept happy-dom route rows for button-only subjects | **Resolved** — re-verified unchanged and consistent across all four stage artifacts: `cicd-pipeline.md:24-42` (lane row + manifest-row table: `oxpilot-repo-trigger` rebinds `routes/oxpilot.tsrx` → `components/oxpilot/new-session-dialog.tsrx`; `oxpilot-new-session` + `oxpilot-connect` gained on the two discovered files; six button-only subjects explicitly "no new rows … exercised inside `oxpilot.integration.test.ts`"), `infrastructure-design-questions.md` Q3-A `:93-116` + summary `:264-267`, `monitoring-design.md:45-55`, `traceability.json:326` (NFR7.1). |
| R-02 | Minor | `infrastructure-specification.md:75-78` | "A fourth flagged amendment (queue_delete)" misnumbered | Renumber: three amendments, `queue_delete` is #3, `0090` backs #1 | Resolved — re-verified: `:75-78` still reads "Amendment #3 (`queue_delete`)… `0090` is amendment #1's backing db artifact, not a fourth amendment"; amendments table `:69-73` unchanged. |
| R-03 | Nit | `cicd-pipeline.md:6-7,27` | Makefile cite drift | Cite `coverage-contract` :394-395, `coverage-web` :362-372 | Resolved — re-verified: `:27` cites "`make coverage-contract` (`Makefile:394-395`) + `make coverage-web` (`Makefile:362-372`)". |
| R-04 | Nit | `infrastructure-design-questions.md:190-192` | CopyCommand path wrong | `components/settings/copy-command.tsrx` | Resolved — re-verified: Q6-A `:191-192` reads "the settings-surface `CopyCommand` (`components/settings/copy-command.tsrx`)". |
| N-01 | Minor | upstream residual: `functional-design/rules.md:249-255`, `functional-design/functional-design-questions.md:412-417` + `:473-474`, `functional-design/functional-spec.md:564-568`, `nfr-requirements/observability-requirements.md:24`, `nfr-requirements/nfr-requirements-questions.md:222-225` | Superseded 8-subject browser-row enumeration survived verbatim in five upstream spots — codegen reading them could re-derive the rejected plan | Extend the corrigendum to all five (bind rows to gate-discovered surfaces + record the narrowing) | **Resolved** — all five sites verified corrected: (1) `rules.md:249-260` BR11.1 now reads "gains subject-keyed browser rows only on gate-discovered surfaces (`browser-coverage-check.ts:255-259` orphan-fails undiscovered files): `oxpilot-new-session`, `oxpilot-connect`" with the repo-`Select` rebind and the full button-only list riding happy-dom route rows, tagged "narrowed at infra-design review"; (2) `functional-design-questions.md:409-420` Q12-A identical binding + summary `:476-479` "browser rows only on the two discovered surfaces (`oxpilot-new-session` + `oxpilot-repo-trigger` rebound, `oxpilot-connect`)"; (3) `functional-spec.md:564-572` file-deltas row binds `oxpilot-new-session` on `new-session-dialog.tsrx` + `oxpilot-connect` on `connect-dialog.tsrx` only, orphan-fail cited, "Q12-A, narrowed at infra-design review" — one cosmetic residual filed as N-03; (4) `observability-requirements.md:24` NFR6.3 now reads "browser rows only on gate-discovered surfaces — new-session dialog, connect dialog; button-only surfaces ride the happy-dom route rows — the gate orphan-fails browser rows on undiscovered files"; (5) `nfr-requirements-questions.md:218-229` Q7-A binds subject-keyed rows to `oxpilot-new-session`/`oxpilot-connect` only with the orphan-fail mechanism + "narrowed at infra-design review" note. Full-tree sweep (`browser row|browser-coverage|subject-keyed|high-risk surface` across `construction/oxpilot-ui/`, plus subject-key grep for all nine `oxpilot-*` keys intent-wide) finds no surviving superseded enumeration — remaining matches are review-file history, the explicit "earlier 8-row browser set" corrigendum note at `nfr-design-questions.md:255`, and generic non-enumerated phrasings that read correctly under the discovered-only interpretation. |
| N-02 | Nit | `cicd-pipeline.md:26`, `infrastructure-design-questions.md:94-96` | Discovery-primitive enumeration omitted `Switch` and `DropdownMenu` | Add the two primitives (or note the list is abbreviated) | Resolved — re-verified: both enumerations list `Select*`/`Checkbox`/`RadioGroup`/`DialogPortal`/`AlertDialogPortal`/`Switch`/`DropdownMenu`/`form.Subscribe` (all eight `HIGH_RISK_RE` classes). |

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| N-03 | Nit | `functional-design/functional-spec.md:570-571` | The corrected file-deltas row enumerates button-only surfaces as "(consent card, rail collapse, diff card cluster)" — dropping "session/target rows, drawer" from the five-item list used at `rules.md:257-259`, `functional-design-questions.md:417-419`, `observability-design.md:92-93`, `monitoring-design.md:50-51` | Complete the parenthetical (or mark it abbreviated, e.g. "incl.") — same completeness standard applied at N-02 | Open |

### Verification notes (what was checked and held up)

- All five N-01 residual sites now carry the discovered-only binding with the orphan-fail mechanism cited (`browser-coverage-check.ts:255-259` by line ref in rules/questions/spec; mechanism-only phrasing at requirements level) and record the narrowing attribution — the codegen-facing `functional-spec.md` file-deltas list now instructs exactly the rows the gate will accept.
- Sweeps: `grep -rn "browser row|browser-coverage|subject-keyed|browser\.test|high-risk surface" construction/oxpilot-ui/` (78 hits — every live artifact consistent; review files carry the historical record only) and an intent-wide subject-key sweep (`oxpilot-(consent|rail-toggle|diff-card|drawer|session-row|target-row|new-session|connect|repo-trigger)` — the six button-only keys appear only as route-row/happy-dom subjects or in "no new rows" context; never asserted as browser rows).
- `frontend-components.md:46-60` coverage column re-verified: all six button-only components marked "happy-dom subject … inside the route row (button-only — undiscovered)"; browser rows only on the two dialog rows.
- Generic non-enumerated phrasings (`nfr-requirements/traceability.json:81`, `functional-design/traceability.json:490`, `nfr-requirements/tech-stack-decisions.md:26`, `nfr-design/logical-components.md:133,150`, `nfr-design/traceability.json:73,100`, `nfr-requirements-questions.md:55,286`) carry no per-surface claim and read correctly under the discovered-only interpretation — not residual, consistent with iter-3 treatment.
- Stage artifacts internally consistent and unchanged since iter-3 verification: `cicd-pipeline.md:24-42` + `infrastructure-design-questions.md` Q3 `:93-116`/summary `:264-267` + `monitoring-design.md:45-55` + `traceability.json:326` all agree: three subject-keyed browser entries on two discovered files, six button-only subjects on kept happy-dom route rows.
- Prior nit resolutions re-verified in place: amendment numbering (`infrastructure-specification.md:75-78`), Makefile cites (`cicd-pipeline.md:27`), CopyCommand path (`infrastructure-design-questions.md:191-192`), full eight-class primitive enumeration (`cicd-pipeline.md:26`, questions `:94-96`).

**Findings:** 6 prior (6 Resolved, 0 Partial, 0 Worsened), 1 new (Nit, Open). Verdict READY — the N-01 corrigendum is fully propagated and no Blocker/Major issues remain; N-03 is a cosmetic enumeration abbreviation in the spec's parenthetical, fixable at codegen-adjacent cleanup without gate impact.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |

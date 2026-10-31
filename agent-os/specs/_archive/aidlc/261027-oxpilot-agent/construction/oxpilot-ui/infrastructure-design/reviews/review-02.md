### Architecture Review — oxpilot-ui infrastructure-design (iteration 2)

### Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY

**Iteration:** 2

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Major | `cicd-pipeline.md` + `infrastructure-design-questions.md` Q3; residual at `monitoring-design.md:45-48` | Browser-coverage manifest plan pinned rows on files the gate's `HIGH_RISK_RE` doesn't discover → orphan-fail (`browser-coverage-check.ts:255-259`) | Pin browser rows only on discovered surfaces; rebind `oxpilot-repo-trigger`; ride kept happy-dom route rows for button-only subjects | **Partial** — plan-of-record is fixed and verified: `cicd-pipeline.md:24-42` + Q3 `:93-116` + summary `:263-269` consistently scope browser rows to the two discovered surfaces (`new-session-dialog.tsrx` Select*/RadioGroup, `connect-dialog.tsrx` Checkbox) + the `oxpilot-repo-trigger` surface rebind, and the six button-only subjects ride the kept `happy-dom` route rows. Mechanism verified in-repo: `HIGH_RISK_RE` `browser-coverage-check.ts:31-32`, `.tsrx` discovery walk `:132-140`, orphan-fail `:255-259`; `routes/oxpilot.tsrx` is discovered today via `SelectRoot/Trigger/Portal` (`:21-24,197-234`) so the rebind rationale is correct; `AlertDialogPortal` lives inside `AlertDialogContent` (`components/ui/alert-dialog.tsrx:20,37`) so `confirm-dialog.tsrx` stays undiscovered as claimed. Manifest shape verified: `route-coverage.manifest.ts:790-807` binds route→test-file only (no subject level), `browser-coverage.manifest.ts:564-570` holds today's `oxpilot-repo-trigger` row. **Residual:** `monitoring-design.md:45-48` still asserts "the subject-keyed `browser-coverage` rows (new-session, connect two-step, consent, rail collapse, diff card cluster, session/target rows, drawer)" — six of the eight named are exactly the undiscovered subjects `cicd-pipeline.md:42` says must NOT carry browser rows; `traceability.json` NFR7.1 note (`:326`) loosely repeats "carry browser rows". Rewrite that enumeration to match the plan-of-record. |
| R-02 | Minor | `infrastructure-specification.md:75-78` | "A fourth flagged amendment (queue_delete)" misnumbered | Renumber: three amendments, `queue_delete` is #3, `0090` backs #1 | Resolved — `:75-78` now reads "Amendment #3 (`queue_delete`)… the functional design numbers three amendments; `0090` is amendment #1's backing db artifact, not a fourth amendment"; consistent with `functional-spec.md:297-299` ("third proposed C1 amendment") and the amendments table `:69-73` |
| R-03 | Nit | `cicd-pipeline.md:6-7,27` | Makefile cite drift | Cite `coverage-contract` :394-395, `coverage-web` :362-372 | Resolved — verified against `Makefile`: `coverage-web:` `:362` recipe → `:372`, `coverage-contract:` `:394-395`, `web-lint:` `:414` block |
| R-04 | Nit | `infrastructure-design-questions.md:190-191` | CopyCommand path wrong | `components/settings/copy-command.tsrx` | Resolved — file exists; cite at `:190-191` matches |

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| N-01 | Minor | upstream: `nfr-design/nfr-design-questions.md` Q7-A `:245-250`, `functional-design/frontend-components.md` coverage column `:46-60`, `nfr-design/observability-design.md:87-94` | Upstream artifacts still pin the superseded superset — seven gained subject-keyed browser rows incl. `oxpilot-session-row`, `oxpilot-target-row`, `oxpilot-consent`, `oxpilot-rail-toggle`, `oxpilot-diff-card`, `oxpilot-drawer` on undiscovered files. Infra-design narrows correctly per the orphan rule but never names the narrowing as a deviation from the decided Q7-A; codegen reading the upstream coverage column could re-derive the rejected plan | Name the deviation explicitly (one line in `cicd-pipeline.md`'s row table or Q3 noting infra narrows Q7-A's row set to discovered surfaces only) or route a corrigendum to the upstream stage | Open |
| N-02 | Nit | `cicd-pipeline.md:26`, `infrastructure-design-questions.md:94-96` | Discovery-primitive enumeration omits `Switch` and `DropdownMenu` — both are in `HIGH_RISK_RE` (`browser-coverage-check.ts:32`) and the cited `docs/TESTING.md:198` list | Add the two primitives to the enumeration (or note the list is abbreviated); no planned file uses them — completeness only | Open |

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |

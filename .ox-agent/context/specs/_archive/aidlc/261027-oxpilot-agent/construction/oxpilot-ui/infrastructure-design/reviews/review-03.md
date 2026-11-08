### Architecture Review — oxpilot-ui infrastructure-design (iteration 3)

### Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY

**Iteration:** 3

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Major | `cicd-pipeline.md` + `infrastructure-design-questions.md` Q3; iter-2 residual at `monitoring-design.md:45-48` + `traceability.json` NFR7.1 | Browser-coverage manifest plan pinned rows on files the gate's `HIGH_RISK_RE` doesn't discover → orphan-fail (`browser-coverage-check.ts:255-259`) | Pin browser rows only on discovered surfaces; rebind `oxpilot-repo-trigger`; ride kept happy-dom route rows for button-only subjects | **Resolved** — residual verified fixed: `monitoring-design.md:45-55` now scopes subject-keyed `browser-coverage` rows to the two discovered surfaces (`oxpilot-new-session` + `oxpilot-repo-trigger` on `new-session-dialog.tsrx`; `oxpilot-connect` on `connect-dialog.tsrx`), and enumerates the six button-only subjects (consent, rail collapse, diff card cluster, session/target rows, drawer) as exercised inside the kept happy-dom route rows, with the orphan-fail rationale (`browser-coverage-check.ts:255-259`) cited. `traceability.json:326` (NFR7.1) now reads "discovered surfaces carry browser rows, button-only surfaces ride happy-dom route rows via NFR6.3" — matches the plan-of-record at `cicd-pipeline.md:24-42` + Q3 `:93-116` + summary `:264-267`. No straggler assertions of browser rows on undiscovered files remain inside this stage's artifacts (grep-verified). |
| R-02 | Minor | `infrastructure-specification.md:75-78` | "A fourth flagged amendment (queue_delete)" misnumbered | Renumber: three amendments, `queue_delete` is #3, `0090` backs #1 | Resolved — re-verified unchanged: `:75-78` still reads "Amendment #3 (`queue_delete`)… the functional design numbers three amendments; `0090` is amendment #1's backing db artifact, not a fourth amendment"; consistent with `functional-spec.md` §Contract deltas and the amendments table `:69-73`. |
| R-03 | Nit | `cicd-pipeline.md:6-7,27` | Makefile cite drift | Cite `coverage-contract` :394-395, `coverage-web` :362-372 | Resolved — re-verified: `:27` cites "`make coverage-contract` (`Makefile:394-395`) + `make coverage-web` (`Makefile:362-372`)". |
| R-04 | Nit | `infrastructure-design-questions.md:190-192` | CopyCommand path wrong | `components/settings/copy-command.tsrx` | Resolved — re-verified: Q6-A now reads "the `components/ui/*` primitive set plus the settings-surface `CopyCommand` (`components/settings/copy-command.tsrx`)". |
| N-01 | Minor | upstream: `nfr-design/nfr-design-questions.md` Q7-A, `functional-design/frontend-components.md` coverage column, `nfr-design/observability-design.md`; residual at `functional-design/rules.md:249-255`, `functional-design/functional-design-questions.md:412-417` + `:473-474`, `functional-design/functional-spec.md:564-568`, `nfr-requirements/observability-requirements.md:24`, `nfr-requirements/nfr-requirements-questions.md:222-225` | Upstream artifacts still pin the superseded superset — subject-keyed browser rows on undiscovered button-only files; codegen reading upstream could re-derive the rejected plan | Name the deviation explicitly or route a corrigendum to the upstream stage | **Partial** — the three named files are corrected and the narrowing is now on record: `nfr-design-questions.md` Q7-A `:245-258` binds browser rows to gate-discovered surfaces only and records "(corrected at infra-design review — recorded narrowing of this option's earlier 8-row browser set)"; `observability-design.md:87-96` same, citing "Q7-A, narrowed at infra-design review"; `frontend-components.md` `:46-60` coverage column marks all six button-only rows "happy-dom subject … inside the route row (button-only — undiscovered)". **Residual:** the identical superseded enumeration — "one subject-keyed row per new high-risk surface: … consent card, rail collapse, diff card cluster" — survives verbatim in five upstream spots, including the most codegen-facing surfaces: `functional-design/rules.md:249-255` (BR11.1 — the binding decided rule that `observability-design.md:99` cites as basis), `functional-design/functional-design-questions.md:412-417` (decided option Q12-A, BR11's source) + summary `:473-474`, `functional-design/functional-spec.md:564-568` (the file-deltas list codegen executes — literally instructs adding rows that orphan-fail), `nfr-requirements/observability-requirements.md:24` (NFR6.3 — requirement row this stage's traceability cites), `nfr-requirements/nfr-requirements-questions.md:222-225` (decided option Q7-A). Extend the same corrigendum to these five (bind rows to gate-discovered surfaces + record the narrowing), or add an explicit narrowing note in `cicd-pipeline.md`/`infrastructure-design-questions.md` naming BR11.1/FD-Q12-A/NFR6.3 as superseded on this point. |
| N-02 | Nit | `cicd-pipeline.md:26`, `infrastructure-design-questions.md:94-96` | Discovery-primitive enumeration omitted `Switch` and `DropdownMenu` | Add the two primitives (or note the list is abbreviated) | Resolved — both enumerations now list `Select*`/`Checkbox`/`RadioGroup`/`DialogPortal`/`AlertDialogPortal`/`Switch`/`DropdownMenu`/`form.Subscribe` — verified complete against `HIGH_RISK_RE` (`browser-coverage-check.ts:32`, all eight classes) and the cited `docs/TESTING.md:198` list. |

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| — | — | — | None. The iteration-3 edits are internally consistent across all four stage artifacts and the corrected upstream files; the only open item is the N-01 propagation residual above. | — | — |

### Verification notes (what was checked and held up)

- `monitoring-design.md:45-55` enumeration matches the manifest plan in `cicd-pipeline.md:38-42` exactly: three subject-keyed browser-coverage entries on two discovered files (`oxpilot-repo-trigger` rebind + `oxpilot-new-session` gained on `new-session-dialog.tsrx`; `oxpilot-connect` gained on `connect-dialog.tsrx`), six button-only subjects on the kept happy-dom route rows.
- `HIGH_RISK_RE` re-verified at `browser-coverage-check.ts:31-32` (`Checkbox`, `RadioGroup(?:Item)`, `form.Subscribe`, `Select(?:Root|Portal|Trigger)`, `Switch`, `DialogPortal`, `AlertDialogPortal`, `DropdownMenu`); orphan-fail at `:255-259`. The corrected enumeration at `cicd-pipeline.md:26` / `infrastructure-design-questions.md:94-96` is now complete — N-02 closed.
- Upstream corrigendum text verified: `nfr-design-questions.md:245-258`, `observability-design.md:87-96`, `frontend-components.md:44-62` — consistent with each other and with the infra plan; the narrowing is explicitly attributed to this review.
- `infrastructure-specification.md:75-78` (amendment #3 phrasing), `cicd-pipeline.md:27` (Makefile cites), `infrastructure-design-questions.md:190-192` (CopyCommand path) re-verified — prior resolutions hold; no regressions introduced by the iteration-3 edits.
- Generic non-enumerated phrasings elsewhere upstream (`nfr-requirements/traceability.json:81`, `functional-design/traceability.json:490`, `functional-design-questions.md:473-474`, `nfr-requirements-questions.md:283`, `nfr-design/logical-components.md:133,150`) read correctly under the discovered-only interpretation — not counted as residual.
- Stage-internal consistency: `infrastructure-design-questions.md` Q3 `:93-116` + summary `:264-267`, `cicd-pipeline.md` `:24-42`, `monitoring-design.md` `:45-60`, `traceability.json` NFR7.1 all agree on the discovered-only browser-row plan; no contradicting text remains in this stage's own artifacts.

**Findings:** 6 prior (5 Resolved, 1 Partial, 0 Worsened), 0 new. Verdict NOT-READY solely on the N-01 residual — one further upstream propagation pass (functional-design BR11.1/Q12-A/spec-deltas + nfr-requirements NFR6.3/Q7-A) closes it.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |

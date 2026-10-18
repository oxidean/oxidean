**Collaborator:** aidlc-design-agent

## Contribution

1. **[blocking] US7.2 / P2 Ravi — the trust loop has no Must-level landing path.** Every Ravi story that ships in the Must band is inspect-only (US8.4 CLI, transitively US4.2/US5.1); his single route to "consequential machine changes stay human-gated" is `git_export` into the protected PR flow — marked Should Have. If delivery-planning cuts the MVP at Must, the P2 persona promise ("agent work enters the normal protected review flow") is unfulfillable: agent work can be read but never landed through the human gate. Q4 admits this ("export is required for the human-trust loop") but stories.md carries no such caveat. Either elevate US7.2 to Must or annotate the G7 header that the MVP band leaves P2 read-only.

2. **[suggestion] Missing Ava story for point reads — `vcs.file_get` / `vcs.changeset_get`.** P1's goal "read precisely what's needed: files … via API, never via full clone" has no behavioral story. G4 covers log/diff/blame/path_history/tree but not the single most common agent read — file contents at a changeset. Both procedures exist only inside AC8.1.1's enumeration. Add a G4 story (e.g., "read one file or one changeset by id") with wire-level ACs.

3. **[suggestion] US8.2 — forced user framing.** "As Sana, I want every procedure registered in all required places" is a system-integrity invariant, not an operator experience; Sana never perceives `PROCEDURES`, dispatch arms, or `CLI_PROCS`. Reframe as a registration-completeness contract/enabler or fold into US8.1's ACs — the fail-closed coverage test is the honest owner.

4. **[suggestion] US7.3 — a constraint wearing a story costume.** "All Git surfaces work identically" is C3 restated; Ravi performs no action and there is one AC that is a regression invariant, not a behavior. More honest as a compatibility AC attached to US1.1/US7.2 or a cross-cutting note.

5. **[suggestion] US4.4 — wrong persona.** `vcs.tree` exists to serve Ravi's checkout (US6.1) and Sana's export (US7.2); Kit is incidental. Reassign to Ravi ("enumerate a changeset's contents before materializing") or frame as shared derived-data behavior.

6. **[suggestion] US2.7 — granularity goal half-met.** Sana's stated need is "not a flood"; excluding per-object rows helps, but one row per changeset at machine write pace is still a flood. No AC requires rows to carry a distinguishable machine/agent identity for feed filtering, or addresses burst behavior. Add an AC on attribution/filterability or record the accepted risk.

7. **[suggestion] US8.6 persona — Kit's scoped-token want lives in Sana's story.** The read/write `proc_classes` split is Kit's core goal ("a narrowly-scoped token gets exactly read or write"); the story is narrated by Sana. Add a Kit-facing AC ("a read-class PAT is denied on `changeset_create`") or dual-attribute.

8. **[suggestion] US6.x all-Ravi — Kit is also a checkout consumer.** CI building/testing a changeset needs materialized files; today Kit must assemble `vcs.tree` + object GETs while `ox vcs checkout` is framed human-only. Either note Kit's path explicitly or add Kit to US6.1.

9. **[suggestion] US5.2 — agents owning heads.** Story assigns ref namespace management solely to Kit, but Ava creating her own named heads (per-task heads she later CASes) is the natural agent workflow; include Ava or clarify refs are integration-managed only.

10. **[nit] Inconsistent procedure naming in ACs.** AC1.2.1 uses bare `changeset_create`/`ref_update`; elsewhere `vcs.`-prefixed. Standardize on the `vcs.` prefix.

11. **[nit] "ref" vs "named head" vs "head" drift** across US5.1/US5.2 prose and ACs. The minimal-namespace distinction from Git branches is worth one canonical term (e.g., "ref (named head)" on first use).

12. **[nit] `checkout` is a Git-ism.** Acceptable for `ox vcs` (matches Ravi's mental model), but ensure contract-design doesn't let the name imply git-style working-copy state — FR8 scopes it to materialize-to-disk; say "materialize" in AC prose at least once (AC6.1.1 does; keep that phrasing).

13. **[nit] Ava bootstrap sequence is implicit.** Discovering a parent (status → `ref_list` → parent id) is assumed by US2.1 but never stated; one AC line would make the first-commit journey testable end-to-end.

## Positions

- **Persona set is right and realistic.** Four personas with genuine, non-generic pain points — Ava's are the article's thesis verbatim (ref contention, clone-per-commit, wasted object round-trips); Ravi's ("VCS as plumbing, not a second mental model") is the truest line in the doc. P1-primary / P2-trust / P3-ops / P4-thin-machine ranking matches the feature's actual shape.
- **Must-band correctly centers Ava's write path.** US2.1–US2.3 and US3.1/US3.3/US3.4 encode the real experiential differences — idempotent resubmit (US2.2), fork-is-normal (US2.3), and AC3.3.1's honest inversion "the protocol never asks the client to describe what it has." These are the stories that make the feature worth building; preserve their AC wording.
- **AC7.2.2 is the right trust-loop seam.** "The branch move passes through the same protection verdict path as `git push`" is exactly the mechanism that lets Ravi trust the bridge — keep verbatim.
- **US2.7's feed contract is well-shaped.** Exactly-one-row per op, authenticated-identity attribution, no per-object rows, reads and `audit_events` untouched — correct human-legible granularity decisions; my only ask is burst/identity filtering (finding 6).
- **US8.5/US8.6 carry Kit's determinism correctly.** No-cookies, PAT-reuse of sha256+`FailedAuthLimiter`, and the `SessionOnly` fail-closed default are the right authz stories.
- **US1.2 is a model operator story** — disable writes, keep reads, typed errors, `db.not_configured` fail-closed. Small, testable, honest.
- **Capability-cluster grouping works.** Each group keeps at least one machine-consumer story plus the human who owns the flow; the INVEST notes are accurate, and wire-level FR-tagged ACs are what contract-design can consume without re-slicing.

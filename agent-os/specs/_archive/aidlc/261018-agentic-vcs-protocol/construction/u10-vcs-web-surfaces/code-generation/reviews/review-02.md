## Review

**Stage:** code-generation
**Unit:** u10-vcs-web-surfaces
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

- **NIT — `hasCommitOid` doubles as a presence check in the `vcs.ref_update` copy branch.** The create/delete/update discrimination keys on `hasCommitOid(item.before)` / `hasCommitOid(item.after)`, which reads as an OID-shape test but is only used for null/empty/all-zeros detection — `vcs.ref_update` rows carry `cs_<ulid>` values, never git OIDs. Behavior is correct; the name oversells the check. Not blocking.
- **NIT — `vcs.git_export`'s commit link is unreachable on real server data.** `showCommitLink` requires `item.commit_message`, but the C4 activity mapping populates `commit_message` only for `vcs.changeset_create`/`merge`/`rebase` — export rows arrive message-less. The gate itself is correct and contract-required; the test exercises the gate, not a reachable server row. Not blocking.

### Summary

Rebind pass under the restarted code-generation attempt (floor #18) — bound bytes identical to the floor-17 READY verdict. `vcs-settings-panel.tsrx` (status query with `vcs.disabled`→unchecked-default, controlled Checkbox draft + explicit Save submit, pending/archived/non-admin disable + explanations, draft-preserving failure, toast + invalidation on success, `fieldset`/`legend`), `repo-activity-row.tsrx` (seven `vcs.*` copy cases, `isGitOid` gate), activity-route chips on exact `push_type`, settings mount + nav row, browser-coverage manifest entry. DOM-race pin satisfied structurally (`keepMounted` on the shared `CheckboxPrimitive.Indicator`).

Tests: integration 8/8 + 6/6, Chromium 2/2 with `expectNoDomRaces`, web suite 609/609, browser-coverage PASS, lint/format/anchors/route-sync clean, workspace nextest 1478/1478. READY.

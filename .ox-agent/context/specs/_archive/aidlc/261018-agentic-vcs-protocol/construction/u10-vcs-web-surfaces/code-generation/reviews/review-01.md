## Review

**Stage:** code-generation
**Unit:** u10-vcs-web-surfaces
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

- **NIT — `hasCommitOid` doubles as a presence check in the `vcs.ref_update` copy branch.** The create/delete/update discrimination keys on `hasCommitOid(item.before)` / `hasCommitOid(item.after)`, which reads as an OID-shape test but is only used for null/empty/all-zeros detection — `vcs.ref_update` rows carry `cs_<ulid>` values, never git OIDs, so the name oversells what is being checked. Behavior is correct (null `before` → "created", null `after` → "deleted", both set → "updated"); a `isPresent`-style helper would be clearer. Not blocking.
- **NIT — `vcs.git_export`'s commit link is unreachable on real server data.** `showCommitLink` requires `item.commit_message`, but the C4 activity mapping populates `commit_message` only for `vcs.changeset_create`/`merge`/`rebase` — export rows arrive message-less, so no link ever renders for them in production. The gate change itself is correct and contract-required (`cs_` ids must never produce `/commit/{sha}` hrefs, and the git-oid path is preserved for push rows); the test fixture asserting the export-link path exercises the gate, not a reachable server row. Not blocking.

### Summary

Review pass over the u10 web surfaces: `vcs-settings-panel.tsrx` (status query with `vcs.disabled`→unchecked-default discrimination per AC1.2.3, controlled Checkbox draft + explicit Save submit, pending/archived/non-admin disable + explanations, inline error preserving draft on failure, toast + query invalidation on success, `fieldset`/`legend` a11y pattern), `repo-activity-row.tsrx` (seven `vcs.*` copy cases, `isGitOid` 40-hex commit-link gate), the activity route's seven exact-`push_type` chips, settings mount + nav row, and the browser-coverage manifest entry.

Verified: `vcs.repoEnable({owner,name,enabled})` is the only mutation path and is unreachable while `pending` (controls disable), so no mid-save toggle race; post-save `setDraft(res.data.enabled)` correctly avoids the refetch-window snap-back; `onCheckedChange` collapses `boolean | "indeterminate"` to a boolean safely; the DOM-race pin is satisfied structurally — `keepMounted` lives on `CheckboxPrimitive.Indicator` in the shared `components/ui/checkbox.tsrx`; the settings page renders the panel inside the admin-gated content with `archived`/`can_admin` props matching the LFS-panel convention; chips ride the existing `type` query-param handling against the server's exact-match `VCS_ACTIVITY_KINDS` filter; nav anchor id matches the mount div; non-admin is the tested dead-state prong per plan.

Tests: integration 8/8 + 6/6 (panel states, save success/failure/draft-preservation, archived/non-admin, all seven kinds, cs_-no-link + git-oid-link regression), Chromium 2/2 (toggle→submit→repoEnable, save-failure alert, `expectNoDomRaces`), full web suite 609/609, browser-coverage PASS, lint/format/anchors/route-sync clean, workspace nextest 1478/1478. READY.

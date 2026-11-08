## Review
**Verdict:** NOT-READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1

### Resolved (each R-id + one line)
- **R-33** — Fixed: `vcs.blame` note (line 72) now reads "…; no refs → `vcs.not_found`; ≥2 refs without `main` → `vcs.invalid_input` (pass `at` explicitly)" — the designed-for agent-refs-only case is closed, matching the suggested fix verbatim.
- **R-34** — Fixed: `vcs.rebase` op-to-row (line 166) now carries literal `commits_count: 1`, matching `vcs.changeset_create`'s pin and the single-changeset wire shape.
- **R-35** — Fixed: `vcs.diff` input (line 65) now reads `from: changeset_id, to: changeset_id` — the AC4.2.1 "between two changesets" domain is pinned in the type slot.

Regression sweep (all pass): all 8 fenced YAML blocks parse; all 17 FR10.1 names verbatim; 13-tool MCP set intact with correct 5-entry absence list; every R-01..R-32 resolution unchanged (POST route + 200/dedup, 413 `vcs.quota_exceeded`, typed `vcs.conflict`, non-idempotent CAS `expected_old`/`new` + `data:{actual}`, `blob`/`content_b64` distinct with delete exempt, blame pagination, log `from` union, cookie scope routes-only, `git_export` → `vcs.forbidden{verdicts}`, `mode`/`old_path`, merge `a`/`b` + recorded supersession, rebase AC2.5.1 shape, optional `author?`, per-provider C6a–d, route-alias note, WS+`system.manifest`+`rpc_body` caps, `vcs.import_state` mapped, five `OXIDEAN_VCS_*` defaults, C4 nullable push columns, C6 signatures consistent).

### New findings
- **R-36 | minor | §C1 inputs — `vcs.tree` (L61), `vcs.file_get` (L84), `vcs.rebase` (L95)** — The R-35 convention (type slot names the id kind) was applied to `vcs.diff` but not propagated: three input sites still read `changeset_id: string` while nine sibling input slots use the `changeset_id` token (`a`/`b`, diff `from`/`to`, `head`, `expected_old`/`new`, `at: "changeset_id?"`, `id:` union). Most jarring on L95 where `changeset_id: string` sits beside `new_parent_id: changeset_id` in one signature — a reader cannot tell whether the replay target is a looser type (it isn't; AC2.5.1 pins a changeset id). Output-side `string` uses (L48/90/106) are fine — outputs follow the looser bare-field convention throughout. **Fix:** type the three input params `changeset_id`.
- **R-37 | minor | §C4 `op-to-row` `vcs.git_export` (L169)** — The row omits `before_oid`, but the stated NULL convention (L159, "ref-free ops write NULLs") only covers ref-*free* ops; `git_export` carries `ref_name: "<branch>"`, so its `before_oid` disposition is unaddressed — NULL vs. the branch's prior oid (the `record_ref_updates` push precedent FR16.1 invokes, and export is pinned as "ONE branch move", AC7.2.1). **Fix:** write `before_oid` explicitly — `null` or `"<prior branch oid>"` — or extend the convention line to "omitted columns write NULL".

### Positives
- All three pass-3 fixes land exactly on their pins: R-33's fallback wording matches the suggested fix verbatim, R-34 uses the literal `1` already established by `changeset_create`, and R-35 uses the contract's own `changeset_id` token — zero collateral edits elsewhere in the file.
- The upstream cross-check is airtight this pass: proc classes match AC8.6.1's enumerated tiers, the taxonomy stays within AC8.1.4's contract-design-owned manifest, `ox vcs` count (16 with `show`/`cat` distinct) matches AC8.4.1, and the MCP absence list reconciles cleanly with the documented `vcs_changeset_get` resolution.
- Every one of the ten Q2 parked freezes is concretely pinned in its owning block, and the C6 internal signatures (`merge(ours,theirs)` retained, `rebase(changeset_id, new_parent_id)` realigned) stay consistent with the wire shapes after three edit rounds.

**Bottom line:** all targeted residuals fixed, full regression sweep clean. Two one-line minors remain — R-36 is residue of the R-35 fix class on the last three input sites; R-37 is a stated-vs-omitted gap in the export activity row. One micro-pass clears it.

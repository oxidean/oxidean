## Review
**Verdict:** NOT-READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1

### Resolved (each R-id + one line)
- **R-24** — Fixed: GET 403 gloss now reads `vcs.disabled (never-enabled repo only — enabled:false still serves reads, AC1.2.1)` (line 133), matching the components.md two-case gate; the POST row (line 140) is correctly left unqualified since both flag states deny writes.
- **R-25** — Fixed: `rpc_body` says "C2's POST body limit" (line 120) and the GC grace var says "POST→commit protection window" (line 227); no "PUT" tokens remain in the file.
- **R-26** — Fixed: `vcs.merge` takes `a`/`b` (line 89) and line 92 records the supersession — "field names a/b supersede the components.md internal signature merge(ours,theirs)…pinned MCP example {a,b} (mockups.md:200); internal signature unchanged" — verified against mockups.md:200.
- **R-27** — Fixed: `vcs.rebase` is the pinned AC2.5.1 shape `{changeset_id, new_parent_id}` (line 95); the note (line 98) states single-changeset replay with chain replay as a client loop — the invented range form is gone.
- **R-28** — Fixed: `author?` is optional (line 46) with the note "resolved server-side from the authenticated platform user when absent (FR2.2); verbatim author is meaningful only on the internal import path" (line 47).
- **R-29** — Fixed: "Exactly one of blob…or content_b64…per non-delete change; delete carries neither" (line 47).
- **R-30** — Fixed: descendant/cycle now names `vcs.invalid_input` (line 97), a code present in the enumerated taxonomy (line 28).
- **R-31** — Fixed in part: the `at`-omitted fallback is now stated (line 72) — but see R-33 for a residual hole in it.
- **R-32** — Fixed: C4 consumer column now lists `u10-vcs-web-surfaces, u2-vcs-persistence` (line 14), matching the nullable-push-column landing on `insert_repo_activity` (line 186).

Regression checks all pass: (a) all 8 fenced YAML blocks still parse — every `: `-/`|`/`→`-bearing scalar and prose continuation is inside double-quoted scalars; (b) all 17 FR10.1 names present in prose and YAML keys, none invented; (c) 13-tool MCP set intact with the correct 5-entry absence list; (d) every R-01..R-23 resolution unchanged (POST route + 200/dedup, 413 `vcs.quota_exceeded`, typed `vcs.conflict`, non-idempotent CAS, cookie scope on routes only, `expected_old`/`new`, `mode`/`old_path`, blame pagination, log union cursor, `no_common_ancestor`, `data:{actual}`, five `OXIDEAN_VCS_*` defaults); (e) C6 `merge_engine` now reads `rebase(changeset_id, new_parent_id)` (line 198) — consistent with the wire shape, `merge(ours, theirs)` retained internally per the recorded supersession.

### New findings
- **R-33 | minor | §C1 `vcs.blame` note (line 72)** — The new fallback covers three cases (`main` exists → its tip; exactly one ref → that head; no refs → `vcs.not_found`) but leaves **≥2 refs without `main`** unspecified — a designed-for state, since `main` is not auto-provisioned (AC5.2.1, stories.md conventions) and agent flows create only `agent/*` refs. **Fix:** close the case in one line — e.g. `…; ≥2 refs with no main → vcs.invalid_input (pass at explicitly)` or a deterministic rule.
- **R-34 | minor | §C4 `op-to-row` `vcs.rebase` (line 166)** — `commits_count: "<replayed count>"` is a stale placeholder from the pre-R-27 range form; under the pinned single-changeset replay the value is structurally `1` (as `vcs.changeset_create` already pins literally). **Fix:** write `1` or note the field is constant under the current wire shape.
- **R-35 | minor | §C1 `vcs.diff` input (line 65)** — `from: string, to: string` are bare `string` while the contract's own convention names id kinds in the type slot (`id: "changeset_id|git_oid"`, `a: changeset_id`, log's `from` union). The accepted id domain — changeset ids only (the AC4.2.1 pin), or also ref names/git_oids, and whether the AC7.1.2 lazy-import trigger applies — is unstated. **Fix:** type them `changeset_id` (or the union explicitly).

### Positives
- All nine second-pass fixes land exactly on the pins — R-24's gloss matches the components.md two-case gate verbatim, the merge/rebase wire shapes now match mockups.md:200 and AC2.5.1 with supersessions explicitly recorded, and the author-optionality fix tracks FR2.2's server-side resolution precisely.
- The R-27 propagation was done thoroughly: the internal C6 signature was realigned to `rebase(changeset_id, new_parent_id)` rather than leaving the invented `rebase(onto, range)`, so wire and seam now agree.
- The "all ten parked items frozen" claim in Open questions now fully verifies — every Q2 item (status schema, ref args, conflict payload, diff bounds, file_get cap, import bound/cursor, activity nullability, 13th MCP tool, env defaults, RPC/route caps) is concretely pinned in its owning block.

**Bottom line:** zero majors and no regressions; three minors remain, each a one-line precision fix (R-33's uncovered blame-fallback case is the most substantive — a Bolt on an agent-refs-only repo would otherwise guess). One micro-pass clears it.

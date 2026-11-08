## Review
**Verdict:** NOT-READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1

### Resolved (prior findings confirmed fixed)

- **R-01** — `vcs.no_common_ancestor` restored to the taxonomy and per-procedure errors (merge_base line 80, merge line 90, rebase line 95, taxonomy line 28).
- **R-02** — C2 upload is now `POST /api/vcs/{o}/{r}/objects` → `200 {"ok":true,"data":{"hash":"sha256:…"}}`, server-computed hash, dedup no-op (lines 122, 132–134) — exactly the FR11.2/mockups pin.
- **R-03** — `413 → vcs.quota_exceeded` pinned (line 138); `vcs.too_large` retained but correctly rescoped to RPC-side caps (line 28).
- **R-04** — `DefaultBodyLimit` raised *above* `OXIDEAN_VCS_MAX_OBJECT_BYTES` so the typed 413 is reachable, citing the real LFS precedent (`app.rs:389` verified: `DefaultBodyLimit::max(2 GiB)`) (line 139).
- **R-05** — `vcs.conflict` is now a typed error envelope with `data:{conflicts:[{path,kind,detail?}]}`, explicitly "never an `ok:true` result" (lines 28, 90, 95).
- **R-06** — `vcs.ref_update` declared intentionally NON-idempotent; retried landed call → `vcs.ref_conflict` (lines 30, 104).
- **R-07** — `changeset_create` carries both `blob?: sha256` and `content_b64?: string` as distinct fields (line 46–47).
- **R-08** — `vcs.blame` accepts `limit`/`cursor` and returns `cursor` (lines 70–71).
- **R-09** — `vcs.log` `from` is the `ref_name|changeset_id|git_oid` union with the lazy-import note (lines 56–58).
- **R-10** — Auth scope note freezes cookie denial to `/api/vcs/*` object routes only; RPC procedures accept session cookies for the web toggle (`credentials:"include"` verified at `packages/api-client/src/index.ts:3725`) (line 5).
- **R-11** — `vcs.git_export` protection denial is the `vcs.forbidden` error with `data:{verdicts}` (line 114).
- **R-12** — `vcs.file_get` output includes `mode` (line 84).
- **R-13** — `changeset_get`/`log` manifest entries carry `old_path?` (line 52).
- **R-14** — `ref_update` fields pinned as `expected_old`/`new` with `ref` per ADR-009 (line 102).
- **R-15** — Object-route error tables now carry `401 vcs.unauthenticated`, `403 vcs.disabled|vcs.forbidden`, `404`, `413`; no `410`, no unnamed codes (lines 128–138).
- **R-16** — C6 split into per-provider rows C6a–C6d covering u3→u4, u4→u5/u6/u7, u5→u7, u6→u7 with correct per-unit owners (lines 16–19, 190–197).
- **R-17** — Route aliases marked as `proc_classes` route-alias names that "never appear in PROCEDURES" (`bearer.rs:735` convention verified — `authorize_pat_on_repo` exists there) (line 140).
- **R-18** — `/api/rpc/ws` WS variant + `system.manifest` advertisement now in C1 row and prose; `rpc_body` cap rule added (lines 11, 26, 117; all verified against the codebase).
- **R-19** — `vcs.ref_conflict` carries `data:{actual}` (lines 28, 104).
- **R-20** — `vcs.import_state` mapped to `git_import` malformed-cursor / concurrent in-progress import (lines 28, 109).
- **R-21** — All five `OXIDEAN_VCS_*` defaults frozen in the operator table (lines 222–226).
- **R-22** — All eight fenced YAML blocks audited line-by-line: every `|` union, `: `-bearing scalar, and prose continuation is now inside double-quoted scalars; all blocks parse.
- **R-23** — `vcs.diff` uses multi-value `paths: "[string]?"` with the repeatable `--paths` note (lines 65, 67).

Cross-checks also pass: all 17 FR10.1 names appear exactly, none invented; 13 MCP tools match the resolved set with the correct 5-entry absence list; proc classes match the AC8.6.1 three-tier split; pagination convention is uniform (`limit`/`cursor` in → `cursor` out); `deny_unknown_fields` confirmed absent from `crates/` supporting the additive-fields claim.

### New findings

- **R-24 | major | §C2 object-routes table (line 130)** — The GET error row reads `"403": "vcs.disabled (never-enabled/disabled repo) | …"`. The "disabled repo" prong contradicts the pinned two-case gate the contract itself adopts (line 43) and AC1.2.1, which explicitly pins object GET succeeding on an `enabled:false` repo — only the absent-flag (never-enabled) case denies reads. Upstream `interaction-spec.md:152` lists `403 vcs.disabled` as a possible *code* without this wrong trigger gloss; the added parenthetical introduces the error. A Bolt built from this would deny disabled-repo GETs. **Fix:** qualify the GET gloss to never-enabled only (e.g. `vcs.disabled (never-enabled repo only — enabled:false still serves reads, AC1.2.1)`); the POST row is already correct since both cases deny writes.
- **R-25 | minor | §C1 `limits.rpc_body` (line 117)** — Stale reference: "same pattern as C2's PUT limit" — C2's upload route is now POST, not PUT (same stale shorthand at line 224 "PUT→commit protection window"). Rename to the POST/body-limit phrasing.
- **R-26 | minor | §C1 `vcs.merge` (line 88)** — `ours`/`theirs` diverges from the pinned mockups wire example `vcs_merge {"a":…,"b":…}` (mockups.md:200) and is inconsistent with sibling `vcs.merge_base`'s `a`/`b` in the same spec. The names do match the `components.md` internal signature `merge(ours, theirs)`, so this is within freeze remit — but per the R-14 precedent the supersession of the upstream example needs recording.
- **R-27 | minor | §C1 `vcs.rebase` (lines 92–95)** — `onto`/`range_from`/`range_to` silently supersedes AC2.5.1's pinned input shape `{changeset_id, new_parent_id}` and the mockup CLI `ox vcs rebase <cs> --onto <cs>` (mockups.md:149). The range semantics are upstream-consistent (`components.md` "replays a chain", C6 `rebase(onto, range)`), but the degenerate single-changeset mapping (how `rebase <cs> --onto <cs>` expresses itself) is undocumented and the supersession unrecorded.
- **R-28 | minor | §C1 `vcs.changeset_create` (line 46)** — `author: {name, email}` is a *required* field, but FR2.2 resolves author from the platform user record when the author is a platform user (all RPC callers are), and both upstream call examples (mockups S2.2/S3:196) omit it. Required client-supplied author also permits arbitrary attribution spoofing. Make it optional with server-side resolution (verbatim author being meaningful only on the internal import path), or record the divergence.
- **R-29 | minor | §C1 `vcs.changeset_create` note (line 47)** — "exactly one of `blob` or `content_b64` per change" doesn't exempt `delete` entries, which carry neither. Reword to "exactly one for non-delete changes; delete carries neither."
- **R-30 | minor | §C1 `vcs.rebase` errors (line 95)** — "typed fail on descendant/cycle target (AC2.4.3)" names no code; the taxonomy is declared enumerated/wire-stable, so map it (e.g. `vcs.invalid_input`) rather than leaving it open.
- **R-31 | minor | §C1 `vcs.blame` (line 70)** — `at: "changeset_id?"` is optional with no stated fallback anchor; fresh repos may have no refs to default against. State the default or make it required.
- **R-32 | minor | Contracts table C4 row (line 14) vs §C4 heading (line 154)** — Heading says "u7 → u10/u2" but the table's Consumer column lists only `u10-vcs-web-surfaces`; the nullable push-column decision lands on U2's `insert_repo_activity` façade signature (C5 already consumes it, line 183). Add u2 to the consumer column or reconcile the heading.

### Positives

- All 23 prior findings are resolved *and* verified against the real code (route shapes, `bearer.rs:735`, `app.rs:389`, `credentials:"include"`, `/api/rpc/ws`, `system.manifest`) — the C2 rewrite now matches the pinned POST/200/`quota_exceeded` contract exactly, including the body-limit-above-cap ordering rule.
- The R-10 fix does what the first review asked — it *resolves* the upstream cookie-scope ambiguity explicitly in a labelled scope note rather than propagating it, which unblocks the U10 `VcsToggle` path.
- The C6 split into per-provider rows (u3/u4/u5/u6 each owning its interface) cleanly satisfies both the "provider owns its spec" rule and the DAG's u3→u4/u4→u5/u4→u6 edge coverage, and every YAML block now parses — unions, prose, and embedded colons all correctly quoted.

**Bottom line:** one remaining wire-contract defect (R-24's denial-scope gloss on the pinned GET route — same severity class as the original R-10) plus eight minors, all one-to-two-line fixes. One more pass should clear it.

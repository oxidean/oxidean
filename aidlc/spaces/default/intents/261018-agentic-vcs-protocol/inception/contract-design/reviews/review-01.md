## Review

**Stage:** contract-design
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location |
|----|----------|---------|----------|
| R-01 | major | Error taxonomy drops the pinned `vcs.no_common_ancestor` code (AC8.1.4 enumeration, AC2.4.3 merge edge, AC4.2.2, mockups.md:186). `vcs.merge_base` returns `null` instead and merge/rebase spec no no-ancestor path — the code must be restored or a superseding decision recorded | contract-summary.md C1 line 21, 34 |
| R-02 | major | Object-upload route contradicts the frozen external surface: pinned `POST /api/vcs/{o}/{r}/objects` → `200 {"ok":true,"data":{"hash":…}}` (FR11.2, AC3.1.1, interaction-spec.md:153, mockups.md:223) is replaced by `PUT …/objects/{hash}` → `201 {stored:true}` + `400 hash-mismatch` — a client-asserted-hash redesign that was never a parked question | §C2 lines 52, 56–57 |
| R-03 | major | `413 vcs.too_large` for oversize PUT contradicts pinned `413 vcs.quota_exceeded` (AC3.1.3, AC3.5.3 "object size limits … enforced with vcs.quota_exceeded", mockups.md:225). `vcs.too_large` may be added to the taxonomy, but the pinned mapping must stand | §C2 line 57 |
| R-04 | major | `DefaultBodyLimit = OXIDEAN_VCS_MAX_OBJECT_BYTES` makes the typed oversize error unreachable — Axum rejects over-limit bodies with an untyped 413 before the handler runs. The LFS precedent (app.rs:389, `DefaultBodyLimit::max(2 GiB)`) sits deliberately ABOVE the semantic check; the body limit needs headroom over the object cap | §C2 line 52 |
| R-05 | major | Merge/rebase conflicts modeled in-band (`{status: clean|conflict, conflicts?}` under `ok:true`) contradict the pinned error model — `vcs.conflict` typed envelope with `data.conflicts` (interaction-spec.md:144; mockups.md:144 `ox: vcs.conflict:` stderr + exit 1, line 201 `isError {code:"vcs.conflict",…}`) | §C1 lines 36–37 |
| R-06 | major | `vcs.ref_update` listed among idempotent procedures ("same `expect` retry → same verdict") contradicts AC5.1.3: CAS is intentionally non-idempotent — a retried landed call fails `vcs.ref_conflict` | §C1 line 21 |
| R-07 | major | `vcs.changeset_create` `changes[]` carries only `blob: sha256` — the pinned inline-contents transport field is missing (AC2.1.2 "two transports are distinct request fields"; mockups.md:72 `@file:` blobspec) | §C1 line 27 |
| R-08 | major | `vcs.blame` has no `limit`/`cursor` inputs and no `next_cursor` output — pagination for `vcs.blame` is pinned by AC4.1.2 and interaction-spec.md:159 | §C1 line 32 |
| R-09 | major | `vcs.log` `from?: changeset_id` is narrower than the pins: mockups.md:214 uses `"from":"main"` (a ref name) and AC7.1.2/components.md pin the lazy-import trigger on git-oid misses for `vcs.log` — needs a `ref|changeset_id|git_oid` union | §C1 line 29 |
| R-10 | major | "PAT/OAT only; cookies never authorize" applied to all 17 RPC procedures breaks U10: `VcsToggle` calls `vcs.status`/`vcs.repo_enable` through `@oxidean/api-client` with `credentials:"include"` (session cookie — packages/api-client/src/index.ts:3725) and `authorize_rpc` passes cookie contexts (rpc.rs:507–509). FR14.1/US8.5 pin cookie denial to `/api/vcs/*` routes only. Upstream U7/components.md phrase it broadly — the stage should have resolved scope, not propagated it | §C1 line 21 |
| R-11 | major | `vcs.git_export` returns `protection:{allowed:bool,verdicts}` in-band under `ok:true`; pinned protection denial is the `vcs.forbidden` error (mockups.md:176 `ox: vcs.forbidden: branch main is protected`; AC7.2.2 "block the export exactly as they block a push") | §C1 line 41 |
| R-12 | major | `vcs.file_get` output drops the pinned `mode` field (AC4.5.2 "bytes + mode") | §C1 line 35 |
| R-13 | major | `vcs.changeset_get`/`vcs.log` manifest entries `{path,kind,mode,blob}` drop `old_path` — renames cannot "read back identically" (AC2.6.2, FR2.5) | §C1 lines 28–29 |
| R-14 | minor | `ref_update` field names `expect`/`target` and the `"absent"` sentinel diverge from the mockups' concrete wire example `expected_old`/`new` (mockups.md:217) and AC5.1.1 `{name,expected_old,new}`. Within freeze remit (ADR-009 pins only `ref`), but the supersession isn't recorded and upstream docs go stale | §C1 line 39 |
| R-15 | minor | C2 error fragments omit pinned `401 vcs.unauthenticated` on both routes and `403 vcs.disabled` for never-enabled repos (AC1.2.3, mockups.md:224); adds unpinned `410`; `400 hash-mismatch` names no code (presumably `vcs.invalid_input` — say so) | §C2 lines 56–57 |
| R-16 | minor | Contracts table undercovers unit-boundary edges: u3→u4 (ObjectLink in create tx, blob verify), u4→u5 (merge write-through/merge_base), u4→u6 (import creates changesets) have no rows — C6's Consumer column lists only u7; C6's single owner (u4) also claims u5/u6-provided interfaces, clashing with "provider owns its spec" | §Contracts table + §C6 |
| R-17 | minor | `vcs.object_get`/`vcs.object_put` are proc_classes aliases, not RPC procedures — the convention exists (bearer.rs:735 maps routes to names) but inside the `vcs.*` namespace they'll read as procedures in denial text/audits; mark them route-aliases never in PROCEDURES | §C2 line 52 |
| R-18 | minor | C1 omits pinned transport surfaces: `/api/rpc/ws` WS variant (AC8.1.1), `system.manifest` advertisement (FR10.3), and the RPC request-body cap that bounds inline blob contents (AC2.1.2 — today Axum's 2 MiB default, unfrozen) | §C1 lines 3, 21 |
| R-19 | minor | `vcs.ref_conflict` `data:{actual}` payload pinned upstream (interaction-spec.md:144, mockups.md:218) is not specified | §C1 line 21 |
| R-20 | minor | `vcs.import_state` added to the taxonomy but no procedure/behavior is mapped to raise it | §C1 line 21 |
| R-21 | minor | `OXIDEAN_VCS_*` defaults only partially frozen — `MAX_OBJECT_BYTES` = 256 MiB is stated; `OBJECTS_DIR`/`GC_INTERVAL_SECS`/`GC_GRACE_SECS`/`REPO_QUOTA_BYTES` defaults are absent, so the open-questions claim "all ten parked items frozen" overstates | §C2 line 52 + Open questions |
| R-22 | minor | YAML spec blocks won't parse: `mode: "100644"|"100755"` adjacent scalars (line 27), `expect: "absent"|changeset_id` plus prose after the flow map's `}` (line 39), quoted path followed by `→` continuation (lines 56–57), `ox: <code>: <message>` containing `: ` inside a plain scalar (line 127) — quote whole values or mark as pseudocode | §C1/C2/C8 yaml |
| R-23 | minor | `vcs.diff` `path?` is singular; the pinned `--paths` filter is a repeatable multi-path list | §C1 line 31 |

### Summary

### Summary

The contract catalog is structurally sound — all nine contracts trace to DAG edges/surfaces, the 17 procedure names match FR10.1 exactly with nothing invented, the 13-tool MCP resolution and the `insert_repo_activity` nullability call land correctly against the real code, and the ownership/breaking-change policy coheres with `wire_compatible`. However, the C1/C2 spec blocks contradict a dozen frozen upstream pins: a dropped error code, an unpinned rewrite of the object-upload route, in-band results where error envelopes are pinned, a wrong idempotency claim for ref CAS, missing pinned fields (`mode`, `old_path`, blame pagination, inline blob transport), a too-narrow `vcs.log` cursor, and a cookie-denial over-generalization that would dead-end the U10 web toggle. Several YAML fragments also won't parse. Iterate once against the pinned AC/mockup surfaces before freezing.

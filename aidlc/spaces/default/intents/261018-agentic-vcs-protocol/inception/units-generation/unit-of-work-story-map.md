# Unit-of-Work Story Map — Agentic VCS Protocol

Every `USx.y` from `inception/user-stories/stories.md` mapped to implementing units. Multi-unit rows reflect real cross-surface stories (procedure + consumer UI, or engine + host wiring); the **primary** unit (bold) owns the behaviour, secondary units own their surface's share.

| Story | Primary Unit | Also in | Notes |
|-------|-------------|---------|-------|
| US1.1 Enable VCS surface | **U7** (vcs.repo_enable + flag gate) | U10 (toggle), U1 (DTO) | two-case flag semantics live in U7 |
| US1.2 Disable writes, keep history | **U7** (write-only denial) | U10 (toggle state), U1 | reads still succeed |
| US1.3 Repo lifecycle carries VCS data | **U2** (cascade DDL) | U3 (objects dir), U7 (factory-reset sweep) | archive/copy/delete coverage |
| US2.1 Commit without a ref | **U4** (create path) | U7 (procedure), U1 | WAL semantics |
| US2.2 Safe retry/idempotent resubmit | **U4** (idempotency key) | U7 | byte-identical input → same id |
| US2.3 Concurrent same-parent commits | **U4** (create tx + upsert index) | — | AC2.3.1 invariant |
| US2.4 Server-side merge | **U5** | U7 | typed conflict envelope |
| US2.5 Server-side rebase | **U5** | U7 | replay + same conflict shape |
| US2.6 Rich file entries | **U4** (FileChange modes/symlinks) | U1 (DTO) | mode bits on the wire |
| US2.7 Activity-feed rows | **U7** (ActivityRecorder + VALID_PUSH_TYPES) | U10 (render/chips), U2 (insert signature) | op-granular; kinds from U1 constants |
| US3.1 Upload objects once | **U3** (PUT + dedup) | U7 (route + body limit) | temp→rename→row |
| US3.2 Fetch objects by hash | **U3** (GET stream) | U7 (route) | octet-stream success / JSON error |
| US3.3 Want-based fetch | **U3** | U7 | no have-negotiation |
| US3.4 Sparse fetch by default | **U8** (manifest filter client-side) | U3 (object GETs) | client-side resolution, not a server op |
| US3.5 Operator-bounded storage + GC | **U3** (quota + GC) | U7 (job spawn + config docs) | env vars land here |
| US4.1 History walk | **U4** (ReadIndex graph walk) | U7 | cursor pagination |
| US4.2 Diff + merge-base | **U4** | U7 | diff boundedness = contract-design |
| US4.3 Path history + blame | **U4** | U7 | per-line attribution rows |
| US4.4 On-demand tree materialization | **U4** (manifest listing) | U7 | vcs.tree |
| US4.5 Point reads by id | **U4** | U7 | changeset_get/file_get (file_get cap = contract-design) |
| US5.1 Advance a ref atomically | **U4** (RefRegistry CAS) | U7 | expected_old semantics |
| US5.2 Minimal ref namespace | **U4** | U7 | no auto-provisioned heads |
| US6.1 Sparse checkout to disk | **U8** (WorkingCopy) | — | non-empty dir typed local error |
| US6.2 Bounded shared object cache | **U8** | — | hash-verified, config client-local |
| US6.3 Faithful file attributes | **U8** | — | mode/symlink fidelity |
| US7.1 Import Git history | **U6** (explicit + lazy chain import) | U7 (trigger seam), U4 | deterministic ids, ImportLink queryable |
| US7.2 Export changeset chain to Git | **U6** (export + hook-driven ref move) | U7 (ProtectionChecker inject) | identical verdicts to git push |
| US7.4 Interop fidelity/convergence | **U6** | U4 | round-trip invariants |
| US8.1 `vcs.*` RPC family | **U1** (DTOs/proc_classes) | U7 (dispatch) | 17 procedures |
| US8.2 Registration completeness | **U1** (rpc-gen + manifests + versioning.toml) | U7 (route nest) | fail-closed coverage test extends |
| US8.3 MCP tools | **U9** | U7 (Access gating) | 12 tools, owner/name args |
| US8.4 `ox vcs` CLI family | **U8** | U1 (wire types) | 16 commands, Out::Rpc, exit codes |
| US8.5 Headless auth, no cookies | **U7** (transport denial) | — | PAT/OAT only on `/api/vcs/*` |
| US8.6 Fail-closed authorization | **U7** (classes + SessionOnly default) | U1 (proc_classes) | explicit classification per procedure |

## Story implementation order within units

- **U4:** US2.1/2.6/2.2 (create + entries + idempotency) → US5.1/5.2 (refs) → US2.3 (concurrency) → US4.1–4.5 (index reads) — write path before derived reads.
- **U7:** flag gate + authz classes (US8.6/8.5) → read procedures → write procedures → activity (US2.7) → lazy-import trigger (US7.1 share).
- **U8:** `checkout`/WorkingCopy (US6.1–6.3, US3.4) after the rest of the command surface (US8.4) — the client-only feature compiles alone.
- Other units are single-concern; order within is unconstrained.

## Coverage verification

- 34 stories declared (US1.1–US8.6; G7 = US7.1/US7.2/US7.4 — US7.3 folded into US7.2's checklist upstream); all 34 assigned — zero GAPs.
- All 10 units carry at least one story (primary or named share): U1 {1.1,1.2,2.1,2.6,8.1,8.2,8.4,8.6}, U2 {1.3,2.7}, U3 {1.3,3.1,3.2,3.3,3.4,3.5}, U4 {2.1,2.2,2.3,2.6,4.1,4.2,4.3,4.4,4.5,5.1,5.2,7.1,7.4}, U5 {2.4,2.5}, U6 {7.1,7.2,7.4}, U7 {1.1,1.2,1.3,2.1,2.2,2.4,2.5,2.7,3.1,3.2,3.3,3.5,4.1,4.2,4.3,4.4,4.5,5.1,5.2,7.1,7.2,8.1,8.2,8.3,8.5,8.6}, U8 {3.4,6.1,6.2,6.3,8.4}, U9 {8.3}, U10 {1.1,1.2,2.7}.
- Cross-cutting stories (spanning units): US1.1/1.2/1.3, US2.7, US3.1–3.4 (store + route/cli), US7.1/7.2 — each has a single primary owner; secondary cells name the surface share only.

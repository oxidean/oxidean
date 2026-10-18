## Review

**Stage:** functional-design
**Unit:** u4-vcs-changeset-core
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| R-01 | blocker | 0054 migration now ships in all three dialect dirs — real ALTERs mysql/ (`index_key`→VARCHAR(560) + `utf8mb4_bin` on index_key/refs.name/file_changes.path), no-op files postgres/+sqlite/; `migration_parity` (migrate.rs:51-69) + FR15.5 satisfied; PK math (36+64+560)×4=2640<3072. | rules.md BR4.19; questions.md Q3 | Resolved |
| R-02 | major | `update_for(id, committed_at, &[FileChangeInput])` pinned identically in Q8 + WF1.3; output struct matches `VcsDerivedIndexUpsert` (vcs.rs:162-167). | questions.md Q8; spec WF1.3 | Resolved |
| R-03 | major | Canonical orientation pinned (mark `lo`, BFS levels from `hi`); call-order-independent, matches sorted cache key; honest "minimal-BFS-distance, may be dominated" claim. | questions.md Q4; spec WF8.1; BR4.14 | Resolved |
| R-04 | major | `message` ≤65535B + `parents` ≤64 → `InvalidInput`; defeats silent TEXT truncation under `INSERT IGNORE` (vcs.rs:1114, mysql 0053:37-39). | questions.md Q2; BR4.4 | Resolved |
| R-05 | major | Ref resolution = match-or-exhaustion scan, no `name ≥ target` early-stop; PG locale-collation rationale recorded. `vcs_list_refs` is `ORDER BY name` + `name > cursor` (vcs.rs:1542-1614) — scan is the only safe shape. | questions.md Q6; spec WF3.4 | Resolved |
| R-06 | minor | Tree scan starts `cursor=Some("{cs}\x01")` + payload-prefix filter — effectively inclusive under exclusive `>` bound. | spec WF5.2 | Resolved |
| R-07 | minor | `get` → `Result<ChangesetRecord,…>`, miss → `ChangesetNotFound`; consistent Q8/entities/WF2.1. | Q8; spec WF2.1 | Resolved |
| R-08 | minor | Deps list names `dep:serde`, `dep:serde_json`, `dep:base64`, `dep:hex` — verified absent from Cargo.toml. | questions.md Q8 | Resolved |
| R-09 | minor | Uniform cursors pinned: log `{"n"}`, keyset reads `{"k":last_index_key}`. | questions.md Q8; entities | Resolved |
| R-10 | minor | `committed_at` DATETIME-range bound + u6 tz-offset handoff note (AC7.4.1 byte-identical round-trip foreclosed at this schema). | questions.md Q2 | Resolved |
| R-11 | minor | Id-shaped ref names rejected (Q6, BR4.9, WF3.4). | BR4.9 | Resolved |
| R-12..R-15 | nit | CHAR(29)/83-char literals, unpaginated `vcs_list_file_changes` note, `payload.path == arg` + rename-row payload semantics, `RefConflict{actual: String}` — all applied and verified. | various | Resolved |
| R-16 | nit | FR2.4/AC2.4.1 added to upstream_ids + coverage; WF6.2 pins candidate-set source. | traceability.json; WF6.2 | Resolved |
| N-01 | minor | `merge_base` mark-set seed ambiguous — "mark `lo`'s ancestors" doesn't state `lo` itself is marked; strict reading breaks the dominant fast-forward/linear case. Pin "mark `lo` AND its ancestors (BFS seed = {lo})". | Q4; WF8.1; BR4.14 | Carried |
| N-02 | minor | `ref_list` cursor unpinned — uniform pin enumerates tree/path_history/blame/diff but not ref list; `list` returns no `next_cursor`; raw `name >` pass-through can never produce `CursorMalformed`. | Q8; entities; WF3.2 | Carried |
| N-03 | minor | `{"k":last_index_key}` doesn't literally apply to `diff` (computed candidate paths, not index rows) and under-pins `blame` (a ≤32KB shard can hold >500 entries — a mid-shard page can't resume with shard-granularity keys). Pin per-read payloads (diff → last emitted path; blame → shard key + `{k,off}` or whole-shard pages ≤500). | entities; WF6.2, WF7.2 | Carried |
| N-04 | minor | WF6.2's candidate walk unbounded — every changeset base→tip × unpaginated `vcs_list_file_changes`; no visited cap (contradicts NFR1 "none unbounded-by-accident"). Pin visited-set cap → `FrontierTooLarge` (unit's own idiom). | WF6.2; traceability NFR1 | Carried |
| N-05 | minor | BR4.4 doesn't pin manifest-level uniqueness — duplicate `path` in one changeset → mid-tx PK error → `Internal` not `InvalidInput`; `old_path == path` rename → LWW self-collision on path_touch. Pin "manifest paths unique; `old_path` ≠ `path` → `InvalidInput`". | BR4.4 | Carried |
| N-06 | nit | entities.md:36 self-contradictory — NULL-head rows "arise only via new=None write which deletes rather than nulls" → intended "never arise". | entities | Carried |
| N-07 | nit | entities.md:9 "CHAR(29) PG/SQLite" — sqlite declares `id TEXT` (0053:31). | entities | Carried |
| N-08 | nit | Naming drift: `NoCommonAncestor` (Q4) vs `BaseNotFound` (entities/WF8.1) — one variant, two names. BR4.8 "(actual=null ⟺ absent)" stale now that `RefConflict` carries `actual: String`. | Q4; entities; BR4.8 | Carried |
| N-09 | nit | `ObjectNotFound{hash}` unpopulatable from bare `vcs.not_found:object` sentinel — population path unpinned (post-sentinel probe of candidate hashes via `vcs_get_object`). | entities; Q7 | Carried |
| N-10 | nit | entities.md `mode` example lists `33177` — BR4.4's pinned set {33188,33261,41471,16384} rejects it. | entities; BR4.4 | Carried |
| N-11 | nit | "key order IS chronological" overstates at second precision — same-second touches order by cs id (content hash). Pin "(second-precision; same-second ties by id)". | entities; WF7.1 | Carried |
| N-12 | nit | FR15.5 invoked by BR4.19 but absent from traceability `upstream_ids`. | traceability.json | Carried |
| N-13 | nit | Blame shard payload omits `path` — `payload.path == arg` semantic guard has no counterpart on blame-cache reads (2^-128 window; add `path` to shard payload or state acceptance). | Q3; entities | Carried |

### Summary

All sixteen iteration-1 findings resolved and re-verified against code (`migration_parity`, CAS arms, `query_derived_index` surface, `INSERT IGNORE`/`STR_TO_DATE` coercion, `is_changeset_id`, dep list, id literals). No internal contradictions introduced; amended `update_for` signature is a conformant refinement of C6's schematic. Thirteen new findings — all minor/nit, none unimplementable — carried to code-generation per binding discipline; N-01 (merge_base seed={lo}) and N-05 (manifest path-uniqueness) are the two worth folding into the plan.

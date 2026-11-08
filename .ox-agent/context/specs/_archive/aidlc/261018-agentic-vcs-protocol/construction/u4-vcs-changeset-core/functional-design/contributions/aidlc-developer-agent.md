# Contribution — aidlc-developer-agent

**Agent:** aidlc-developer-agent
**Stage:** functional-design
**Unit:** u4-vcs-changeset-core

## Verified (claim → where verified)

| Claim | Where verified |
|---|---|
| `vcs_create_changeset_tx(changeset, file_changes, object_links, index_upserts)` arg order + in-tx sequence (idem lookup → parent check → object check → insert-if-absent → file changes → links → index upserts) | vcs.rs:914–1058 (PG arm; identical MySQL 1060+, SQLite 1193+); wrapper 2425 |
| `vcs_query_derived_index` = `kind=` + optional **exact** `index_key=` + `index_key > cursor`, `ORDER BY index_key`, clamp 1..=500 — no LIKE/prefix predicate | vcs.rs:2082–2186; wrapper 2518 |
| `vcs_upsert_derived_index_rows` = LWW payload-replace on `(repo,kind,index_key)`, all rows one tx | vcs.rs:1985–2077; wrapper 2509 |
| `vcs_cas_ref_update` absent+delete → `Conflict{actual:None}`; `actual=None` ⟺ row absent | vcs.rs:1620–1950; wrapper 2493 |
| `vcs_lookup_import_link(repo, git_oid) -> Result<Option<String>>` | vcs.rs:2237–2268; wrapper 2548 |
| `vcs_changeset_parents` (empty on absent/parentless), `vcs_get_changeset`, `vcs_list_file_changes` (path-ordered), `vcs_list_refs` (keyset `name>cursor`), `vcs_delete_ref -> bool` | vcs.rs:1344–1614, 1954 |
| First-parent fold `tree(cs)=tree(parent[0])+delta(cs)` sanctioned upstream | stories.md AC2.4.1, FR2.4 |
| Four sentinel strings are the only typed decode channel | vcs.rs:20–23 + module doc 5–10 |
| C6 grants `object_store (u3 → u4): get(hash) -> stream` | contract-summary.md:196 |
| AC2.4.1 merge manifest "relative to the first parent"; AC4.1.1; AC4.3.1 no order pinned; AC5.2.2 absent→not_found; AC7.4.2 canonical `Name <email>` | stories.md |
| `vcs_derived_index` PK `(repo_id,kind,index_key)`; `covering_changeset` FK→changesets CASCADE; `uk_vcs_changesets_repo_idem` NULL-distinct; `vcs_file_changes` PK `(changeset_id,path)`, blob RESTRICT | migrations pg/mysql/sqlite 0053 |
| MySQL: `index_key VARCHAR(255)`, `path VARCHAR(512)`, `name VARCHAR(255)`, `author VARCHAR(255)`, `payload TEXT` (64KB), `git_oid CHAR(64)`; **no COLLATE/CHARSET pins anywhere** | mysql 0053 |

## Findings

| ID | Severity | Finding | Location | Recommended fix |
|---|---|---|---|---|
| D4-01 | blocker | `\0` in `index_key` cannot be stored on Postgres (TEXT rejects byte 0x00). All key kinds use `{a}\0{b}`; `path_touch` upserts run inside `create_tx` → every create fails on PG. | entities.md; Q3; BR4.14 | Replace `\0` with `\x01` (legal all dialects, sorts below legal path chars) + "no control bytes" path rule; verify collation ordering or binary-collate `index_key` upstream. |
| D4-02 | blocker | `crockford32_lower` fails `ChangesetId` validation — accepts only **uppercase** Crockford (`0-9 A-H J-K M-N P-T V-Z`). Minted lowercase ids decode-fail at every wire DTO boundary. | Q1/BR4.1 vs vcs_types.rs:156–167 | Encode the 16-byte prefix as canonical **uppercase ULID** packing (128 bits MSB-first, 26 chars); add `ChangesetId::parse` assert in create tests. |
| D4-03 | blocker | MySQL `index_key VARCHAR(255)` can't hold `{path}\x01{cs}` for legal paths (`vcs_file_changes.path` allows 512) → creates fail for long paths. `author VARCHAR(255)` also unbounded in artifacts. | entities.md vs mysql 0053; BR4.4 | Widen `index_key` in a 0054 migration (PK byte math (36+64+560)×4=2640<3072) or cap path; preferred: widen + collate (see D4-10). |
| D4-04 | blocker | Log cursor `{frontier, emitted}` drops the seen set — a parent emitted in page k can be re-pushed and re-emitted in page k+j (same-second commits are normal: `committed_at` round-trips at SECOND precision, id tiebreak is a hash → topology-inverting order common). `emitted:n` field is dead weight. | Q4; WF4 | Cursor = `{n}` alone + **re-walk skipping n pops** — heap is a total order, replay deterministic, never `too_large`; or carry bounded `seen_ids` (rejected: O(emitted)). |
| D4-05 | blocker | No `vcs_get_ref` exists — C5 pins `ref: {list, cas_update, delete}` only. WF3.1c pre-read and WF4.1 point-read have no primitive; traceability claims "zero new facade methods". | WF3.1c, WF4.1; contract C5 | (a) absent→not_found needs no pre-read: facade already returns `Conflict{actual:None}` — map it when `new=None` or `expected_old=Some` (atomic, kills TOCTOU). (b) `log(from=ref)`: pin a `vcs_list_refs` scan — keyset pages, stop at first `name ≥ target`. |
| D4-06 | major | Read entry points never existence-check changeset inputs: `log(from=cs_id)` skips `vcs_get_changeset`; `materialize(absent)` → `[]` ambiguous success (AC4.1.3 violation); `merge_base` on absent → wrong variant `NoCommonAncestor` not `not_found`. | WF4.1, WF5.1, WF8 | `vcs_get_changeset` existence check → `NotFound` at each read entry. |
| D4-07 | major | Blame needs object bytes the boundary excludes: algorithm diffs blob **lines** + stores line content — impossible from hashes. C6 grants `object_store (u3 → u4): get(hash) -> stream`. Same gap for diff `patch`/`totals`. | Q5, BR4.17 vs object_store/mod.rs:311, contract C6 | Amend boundary: "no object bytes on the **create/write path**"; pin `ObjectStore`/`BlobReader` as ReadIndex+text_diff dep; state who computes totals/patches. |
| D4-08 | major | Canonical recipe not byte-exact for U5/U6/U7 convergence: parents_json spacing, manifest top-level shape, `mode` encoding (i64 33188 vs "100644"), `blob` form (bare-hex vs `sha256:`), dedup ordering, author composition, `repo_id` deviation from FR2.2 unrecorded. | BR4.1, Q1 vs FR2.2 | One shared `canonical_changeset_id()/manifest_hash()` fn U5/U6/U7 MUST call; pin serde_json compact fixed structs; mode=i64 octal; blob=bare hex; parents compact deduped; record repo_id deviation (needed: global `id` PK + repo-scoped reads would cross repos). |
| D4-09 | major | `committed_at` format unpinned → silent dialect divergence: MySQL `STR_TO_DATE('%Y-%m-%d %H:%i:%s')` + `INSERT IGNORE` swallows fractional seconds (truncation warning ignored) and git offset forms (+02:00 → NULL → zero-date); PG parses full ISO → different stored value per dialect. Malformed → `Internal` not `InvalidInput`. | Q2/BR4.5 vs vcs.rs:1114–1131 | Pin exactly `%Y-%m-%dT%H:%M:%SZ` (`SecondsFormat::Secs`); normalize offsets→UTC; reject fractions; malformed → `InvalidInput`. |
| D4-10 | major | MySQL collation hazard: no migration sets CHARSET/COLLATE → utf8mb4 `*_ai_ci` case-**insensitive** PK comparisons — `README`/`readme` collide in `index_key`/`refs.name`/`file_changes.path` → upsert-merge → silent row loss. `=`/`>` probes also mis-compare under ci. | mysql 0053 | New 0054 MySQL migration: `COLLATE utf8mb4_bin` on `vcs_derived_index.index_key`, `vcs_refs.name`, `vcs_file_changes.path` (+ widen index_key per D4-03). PG equality is bytewise; SQLite BINARY default — no changes needed there. |
| D4-11 | major | `blame` cache single-row payload can exceed MySQL `TEXT` (64KB) — whole-file line contents in one `(cs,path)` row. | entities.md, Q5 vs mysql 0053:77 | Shard: `{cs}\x01{pathhash32}\x01{chunk:04}`, payload ≤32KB per row. |
| D4-12 | major | `path_history` ordering incoherent: wire request has no anchor — "ancestor position" undefined across divergent branches; index_key-ordered pages + per-page resort = globally unordered response; `PathTouch` needs `committed_at` the payload omits (N+1). | BR4.14, WF7.1 vs vcs_types.rs:766 | Re-key `path_touch` as `{sha256(path)[:32]}\x01{committed_at}\x01{cs}` — chronological within path, keyset cursor honest, hash derivable, fixed ≤82 chars; add `committed_at` to payload. |
| D4-13 | major | `merge_base` under-pinned: heap pop can return ancestor-of-better-base under timestamp skew (second-precision); FIFO-BFS levels is defensible; mark set unbounded (NFR1); absent a/b must be `not_found`. | Q4, WF8.1 | Existence-check both; FIFO-BFS from b; first level containing marked nodes → tie-break `(committed_at,id)`; cap mark set 250k → `too_large`. |
| D4-14 | minor | `LogEntry` omits `manifest` — wire `VcsChangesetPublic` requires it → per-entry `vcs_list_file_changes` needed (≤500/page, facade-bounded). | entities.md | State owner: ReadIndex fetches file_changes per entry. |
| D4-15 | minor | `update_for` signature drifts across artifacts (Q8 vs WF1.3 vs C6 "inside tx" — it's a pure pre-tx producer). | Q8, WF1.3, C6 | Pin `update_for(id, &[FileChangeInput]) -> Vec<VcsDerivedIndexUpsert>`. |
| D4-16 | minor | `log.from` ref-vs-id ambiguity: a ref named `cs_<26>` or 40-hex is legal → precedence unpinned. | WF4.1, BR4.9 | Pin: `cs_`→changeset, 40/64-hex→import link, else ref scan. |
| D4-17 | minor | `path_history(old_path)` misses rename events — `path_touch` keyed on new path only. | entities.md, Q3 | Emit second `path_touch` row keyed `sha256(old_path)` payload `kind:"rename"` carrying real old_path. |
| D4-18 | minor | Materialize atomicity inconsistent: single upsert call is atomic — the "partial entries, no snap" crash story needs chunked `tree_entry` upserts + `tree_snap` in a separate final call. | entities.md:105, WF5.1 | Pin chunked entries + separate final snap call. |
| D4-19 | minor | `index_key=None + cursor` scan shape unstated — probes exact+cursor=None; scans index_key=None+cursor=start. | WF5.2, WF7.1 | Add call-shape note. |
| D4-20 | nit | Sentinel consts are private — decode sites pin the four literals; docstring vcs.rs:5–10 is the contract. | vcs.rs:20–23 | Pin literals in error-mapping module; note non-pub intent. |
| D4-21 | nit | entities: `idem_key required` but column NULL-able (Option); NULL-head ref never inserted (CAS always binds `new`); blame `author` canonical string vs wire `VcsAuthor{name,email}` — u7 re-splits (lossy edge). | entities.md | Adjust attribute text; author re-parse note to u7 handoff. |
| D4-22 | nit | `IndexError::CursorMalformed` vs BR4.15 → `invalid_input` — pick mapping. | entities.md, BR4.15 | Keep internal variant; state it maps to `invalid_input`. |
| D4-23 | nit | `base64` dep missing from Q8 dep list (LogCursor codec). | Q8 | Add `base64` (or hex-encode cursor). |
| D4-24 | nit | Traceability: FR10.1 empty note; FR2.2 should also hit BR4.1; ACs sitting on D4-01..13 marked OK → regrade at-risk; NFR1 "bounded everywhere" overclaims (mark set, blame collect, materialize depth unbounded). | traceability.json | Update statuses/notes. |
| D4-25 | nit | No case needs `idem_key` ≠ canonical hash — keep as forward-compat handle (caller-chosen dedup across recipe versions); 128-bit prefix collision reselects other content's row (returns its id as Ok) — cryptographically negligible, worth a comment. | rules.md, vcs.rs:841 | Pin idem_key = full canonical hex + comment. |
| D4-26 | nit | `committed_at: Option<String>` is the clock-injection seam (tests pass `Some`); `object_store::testutil` reusable; `Utc::now()` inline acceptable — say so. | Q2 | One line pinning test seam. |
| D4-27 | nit | Asymmetric id resolution: `get()` cs_-only, `log()` tri-form — consistent with C6 but unsaid; a dev may wrongly add oid resolution to `get`. | WF2.1 vs WF4.1 | One sentence pinning u7's dispatch. |

## Summary

Artifacts are strong on concept — deterministic content ids, sentinel-exact decode, first-parent fold, LWW-safe kinds, facade call sequence all verified. Failures are encoding/operational detail:

1. `cs_<26>` recipe says lowercase; validator demands uppercase — every minted id decode-fails at wire.
2. `\0` separators unwritable on PG; keys can exceed MySQL VARCHAR(255) — create-path failures.
3. Log cursor loses dedup set → cross-page duplicate emission probable.
4. `vcs_get_ref` does not exist — absent→not_found achievable atomically via existing `Conflict{actual:None}`; ref-resolution needs a spelled-out `vcs_list_refs` scan.
5. Blame unimplementable as scoped — needs `ObjectStore::get` (C6 grants u3→u4); single-row cache exceeds TEXT 64KB.
6. Missing existence checks → AC4.1.3 violations; canonical recipe not byte-pinned; `committed_at` second-precision UTC-only needed for MySQL; MySQL ci collation makes case-variant paths/refs collide; path_history anchor-less ordering; read-shape gaps (manifest in log, committed_at in path_touch).

**Not performed (read-only):** no file edits; collation/`INSERT IGNORE`/`STR_TO_DATE` behaviors verified against docs and code paths, not executed against live DBs — confirm D4-09/D4-10 empirically in dialect CI before final severity call.

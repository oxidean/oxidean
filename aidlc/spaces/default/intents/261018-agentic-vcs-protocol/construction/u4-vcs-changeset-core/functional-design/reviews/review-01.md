## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Stage:** functional-design
**Unit:** u4-vcs-changeset-core
**Verdict:** NOT-READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Recommendation |
|----|----------|---------|----------|----------------|
| R-01 | blocker | "MySQL-only 0054 migration" fails `migration_parity` test (`migrate.rs:52-68` asserts identical filename sets across dialects) and FR15.5 filename parity. | rules.md BR4.19; questions.md Q3 | Ship `0054_*.sql` in all three dirs — real ALTERs in mysql/, no-op comment files in postgres/ and sqlite/. |
| R-02 | major | `update_for(id, &[FileChangeInput])` cannot produce the pinned `path_touch` key — it embeds `{committed_at}` not derivable from id or changes (D4-12 re-key vs D4-15 signature conflict). | questions.md Q8; spec WF1.3; rules.md BR4.12 | Pin `update_for(id, committed_at, &[FileChangeInput])`. |
| R-03 | major | `merge_base` paint-down order-sensitive pre-cache (mark a's vs sorted key `{lo}\x01{hi}`) and can return a dominated base; "all minimal-distance bases" overstates fidelity. | questions.md Q4; spec WF8.1; rules.md BR4.14 | Canonical orientation (always mark `lo`'s ancestors, BFS from `hi`); document pick as "a minimal-BFS-distance common ancestor" (deterministic, may be dominated), or add domination check. |
| R-04 | major | `message`/`parents` MySQL `TEXT` (64KB) under `INSERT IGNORE` silently truncate over-limit values → stored row diverges from hashed input. Same silent-truncation class as D4-09. | mysql 0053:37-39 vs BR4.4 | Pin byte bound on `message` (≤65535B) + parent-count cap → `InvalidInput`. |
| R-05 | major | Ref-resolution "stop at first `name ≥ target`" assumes bytewise `ORDER BY name` — false under PG locale collations (ignores `-`/`.`) → deployment-dependent silent `NotFound`. | spec WF3.4; questions.md Q6 | Do not early-stop: page `vcs_list_refs` until `name == target` or exhaustion. |
| R-06 | minor | `tree` scan bound `"{cs}\x01{path_prefix}"` is exclusive (`>`) — a file whose path equals `path_prefix` exactly is skipped. | spec WF5.2 | Start bound at `"{cs}\x01"` + payload-prefix filter. |
| R-07 | minor | `get` signature returns `Option` (Q8) but WF2.1/entities pin `ChangesetNotFound` — ambiguous miss contract. | questions.md Q8 vs spec WF2.1 | Pick one: `Err(ChangesetNotFound)` (variant exists, wire needs `vcs.not_found`). |
| R-08 | minor | Q8 dep list omits `serde`/`serde_json` — Cargo.toml has neither under any feature. | questions.md Q8 | Add `dep:serde`, `dep:serde_json` under `server`. |
| R-09 | minor | Only `LogCursor` form pinned; tree/path_history/blame/diff cursors unspecified though BR4.15's CursorMalformed mapping implies opaque everywhere. | entities.md; spec WF5–7 | Pin uniform opaque base64 `{k: last_index_key}` for keyset reads. |
| R-10 | minor | `committed_at` has no range bound — MySQL DATETIME spans 1000..9999; out-of-range git dates → mid-tx `Internal`. Also: UTC normalization drops tz offsets — forecloses AC7.4.1 byte-identical round-trip (u6's AC; record for handoff). | questions.md Q2 | Pin DATETIME-range check → `InvalidInput`; record tz-offset loss as u6 handoff note. |
| R-11 | minor | BR4.9 allows refs named `cs_<26>` or 40/64-hex — creatable but unresolvable in `log from` (id-precedence shadows them). | rules.md BR4.9 | Reject id-shaped names → `InvalidInput`. |
| R-12 | nit | `CHAR(32)` id claim — actual `CHAR(29)` PG/SQLite, `VARCHAR(32)` MySQL; "fixed 82-char key" is 83. | entities.md:9; questions.md:41 | Correct literals. |
| R-13 | nit | "`vcs_list_file_changes` per entry, ≤500/page" — facade method is unpaginated; u7's 10k manifest cap is the real bound. | entities.md:53 | Drop the ≤500 annotation. |
| R-14 | nit | `path_history` never pins `payload.path == arg` filter (128-bit hash collision semantic guarantee) or the rename row's payload `path` semantics. | spec WF7.1; questions.md Q3 | Pin filter + rename-row payload `path` = the OLD path. |
| R-15 | nit | `RefConflict{actual:Option<String>}` — every `actual:None` case maps to RefNotFound/Ok per BR4.10 → vestigial Option. | entities.md:65 | Tighten to `actual: String` or note the invariant. |
| R-16 | nit | traceability omits FR2.4/AC2.4.1 which BR4.13 depends on; `diff` candidate-set enumeration ("unioned changed-path set") vague. | traceability.json; spec WF6 | Add FR2.4 as consumed upstream id; pin candidate-set source. |

### Summary

Contribution integration is faithful — every verified facade/schema claim matches the code. One blocker: BR4.19's "MySQL-only" migration collides with the enforced filename-parity test (fix: no-op sibling files). Four majors: `update_for` lacks the `committed_at` input its key grammar requires; `merge_base` order-sensitive pre-cache and can return dominated ancestor; `message`/`parents` TEXT truncate silently; ref-resolution early-stop is collation-fragile on PG. All fixes are small spec amendments — READY after one revision pass.

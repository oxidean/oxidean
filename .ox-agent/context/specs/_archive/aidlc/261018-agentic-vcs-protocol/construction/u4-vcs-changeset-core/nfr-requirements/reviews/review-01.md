## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Stage:** nfr-requirements
**Unit:** u4-vcs-changeset-core
**Verdict:** NOT-READY
**Iteration:** 1

### Verified (claim → where verified)

- **12-method facade surface, `vcs_get_ref` absent** — all pinned names exist: `vcs_create_changeset_tx` `vcs.rs:2425`, `vcs_get_changeset` `:2442`, `vcs_changeset_parents` `:2451`, `vcs_find_changeset_by_idem` `:2459`, `vcs_list_file_changes` `:2473`, `vcs_list_refs` `:2481`, `vcs_cas_ref_update` `:2493`, `vcs_delete_ref` `:2504`, `vcs_upsert_derived_index_rows` `:2509`, `vcs_query_derived_index` `:2518`, `vcs_lookup_import_link` `:2548`, `vcs_get_object` `:2372`. `vcs_get_ref` — zero matches.
- **Sentinels** — four private consts `vcs.rs:20-23`, generated at `create_preflight` `:821/:824` and in-tx `:955-957`, `:1108-1110`.
- **CAS wording (DS-05)** — PG/SQLite fill `actual` via separate `ref_actual_*` SELECT; only MySQL is in-tx atomic (`SELECT … FOR UPDATE` `:1846-1862`). SR4.4's post-CAS-observation wording is accurate.
- **Mode set** — `{33188, 33261, 40960, 16384}` = `0o100644/0o100755/0o120000/0o040000`; SR4.1 correctly supersedes the FD's `41471`.
- **PK math** — `(36+64+560)×4 = 2640 < 3072`; `payload TEXT` = 65535B; no table-level charset in 0053 → explicit `CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL` is required.
- **Deps** — `serde`/`serde_json` workspace deps; `base64 = "0.23"` api pin; `hex`/`ulid` absent workspace-wide; `oxidean-core` dev-dep only — `dep:oxidean-core` genuinely required (`is_changeset_id` private; `ChangesetId::parse` is the oracle).
- **CI/test posture** — `db-dialects` runs only `binary_id(oxidean-db::dialect_probe)`; 0054 ALTERs will execute on real MySQL there (migrate-only, as TD4.8 now honestly claims); `dialect_*` gated-test family (22 files under `oxidean-db/tests/`) is the established convention `dialect_vcs` rides.
- **DS-01..DS-16 integration** — all 16 verified landed and honest; carried N-* rows re-marked faithfully.

### Findings

| ID | Severity | Finding | Location | Recommended fix |
|----|----------|---------|----------|-----------------|
| AR-01 | **major** | **`path_touch` uniqueness is falsified by reachable manifests — the old-path row collides with any other same-changeset entry sharing its path hash.** Keys are `{sha256(p)[0:32]}\x01{committed_at}\x01{cs}`; a rename emits a second row keyed `sha256(old_path)` with the same `cs`+`committed_at`. `{add a}` + `{rename a→b}` (move-and-recreate is semantically legal) or two renames sharing `old_path` produce identical `index_key`s → LWW inside `create_tx` silently merges two logical events — `path_history("a")` loses one, blame's rename chain can miss a hop, no error surfaces. SR4.1 pins `paths unique` + `old_path ≠ path` but not `old_path` disjointness. Falsifies SR4.6's "unique-per-logical-row by construction." | security-requirements.md SR4.1, SR4.6; questions.md Q3; vcs.rs:1997-2074 | Extend SR4.1: "`old_path` values are unique across the manifest" → `InvalidInput`; AND key-segregate the old-path row `{hash32}\x01{ts}\x01{cs}\x01r` (still matched by the `{hash32}\x01` prefix scan, sorts after the plain row) so move-and-recreate stays expressible. |
| AR-02 | **major** | **SR4.11's "the one accepted unbounded-cost residual" is false — `blame` touch-collection is a second unbounded-in-schema path.** WF7.2 collects all `path_touch` rows for a path (+`old_path` rename chain), graph-orders via ancestor walk, opens blobs per touch — O(#touches) memory + O(#touches) `ObjectStore::get` opens per cache-miss call; a hot file is O(history). "Bounded by path history" is a data-bound, not a cap. | security-requirements.md SR4.8, SR4.11; spec WF7.2 | Pin an injectable touches cap → `FrontierTooLarge` (same idiom), and fix the "one accepted" exclusivity claim. Fold in: the `seen`-cap 65536 also forecloses `log` page-out deeper than ~32-64k visited nodes — the register covers the quadratic cost but not the depth ceiling. |
| AR-03 | minor | **NFR7 (resumability) mis-targeted** — points at SR4.6/TD4.3 (collation/migration — NFR6's content). u4's resumability substance is SR4.7 cursor resume + tree_snap-last crash-resume + TD4.8's crash-resume arm. | traceability.json NFR7 row | Retarget to SR4.7 + TD4.8. |
| AR-04 | minor | **NFR8 (observability) omits SR4.12** — the tracing half of NFR8 is the unit's own requirement. | traceability.json NFR8 row | Add SR4.12 to the target. |
| AR-05 | nit | **TD4.1 mislabels `Ref` as a wire type** — no `Ref` exists in `vcs_types.rs` (wire DTO is `VcsRefPublic`; domain `Ref` is the local entity). The dep is still required — `ChangesetId::parse` alone forces it. | tech-stack-decisions.md TD4.1 | Reword: "`ChangesetId` (and the `parse` oracle per SR4.3) come from `oxidean_core::vcs_types`." |
| AR-06 | nit | **`dialect_vcs` placement unpinned** — the `db-dialects -E` filter selects only `dialect_probe`; `vcs-server` sets no `DATABASE_URL`. | tech-stack-decisions.md TD4.8 | Pin the file home: `crates/oxidean-db/tests/dialect_vcs.rs` beside `dialect_probe.rs`, `DATABASE_URL`-gated skip-pass; CI execution optional (db-dialects `-E` widening is a separate decision). |

### Summary

The artifacts are exceptionally well-verified — every facade name, sentinel literal, CAS arm, collation/PK-width computation, schema bound, dep claim, and CI invocation checked against `vcs.rs`, the three 0053 DDLs, `Cargo.toml`s, `vcs_types.rs`, `gc.rs`, `migrate.rs`, `ci.yml`, and the Makefile is accurate. All 16 devsecops findings integrated faithfully.

Two majors block READY — both small spec fixes, not redesigns: **(AR-01)** the manifest-level uniqueness pin misses the `old_path` dimension, leaving a reachable silent LWW merge on `path_touch` keys that falsifies SR4.6's "unique-by-construction" claim; **(AR-02)** the residual register's "one accepted unbounded-cost residual" is falsified by `blame`'s per-call O(#touches) collection — same class the register exists to record.

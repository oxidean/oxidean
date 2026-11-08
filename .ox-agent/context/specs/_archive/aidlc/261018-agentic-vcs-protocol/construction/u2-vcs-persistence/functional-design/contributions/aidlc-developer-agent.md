Subagent agent_id=7cf48a10 completed successfully:

# u2-vcs-persistence functional-design — blind-pass verification findings

Verified against `crates/oxidean-db` real code. Context confirmed correct: `sqlx::migrate!` embeds per-dialect dirs (`src/migrate.rs:5-12`), filename parity is unit-test enforced (`migrate.rs:51-69`), 0053 is the right number (latest = `0052_action_run_head_repo.sql` ×3; the 0041/0042 gap is pre-existing), `VCS_ACTIVITY_KINDS` exists (`oxidean-core/src/vcs_types.rs:73`), `require_pool` → `"database not configured"` String (`lib.rs:150-154`) mapped to `db.not_configured` in oxidean-api (`rpc.rs:562-564` and ~30 handlers), C5 façade list matches `contract-summary.md:178-187` verbatim.

---

**1. BLOCKER — `RepoActivityRow` read-side widening is U2's per upstream pins but absent from all four artifacts**

- **Artifact:** entities.md:116-120 + rules.md BR2.5 + functional-spec.md Scope/WF5 — all claim "insert_repo_activity signature → `Option<>`, callers pass `Some(...)`, **no behavior change**". They cover only the *insert* side.
- **Reality:** `RepoActivityRow` fields are non-Option (`repo_activity.rs:12-16`: `ref_name/before_oid/after_oid: String`, `commits_count: i64`). `map_activity!` (`repo_activity.rs:33-65`) does `try_get::<String>`/`try_get::<i64>` on those columns — on a NULL row it returns a decode error that propagates (`?`), breaking `list_activity`, `list_by_actor`, `list_by_actor_public` for *all* rows once any ref-free `vcs.*` row exists.
- **Cross-crate blast radius (all stop compiling when the row widens):**
  - `crates/oxidean-api/src/repo/activity.rs:367-372` — `row_to_item` wraps `Some(row.ref_name)`/`Some(short_ref(&row.ref_name))`/`Some(row.commits_count)` (u1 pinned the de-wrap to u2 — `u1-.../nfr-design/logical-components.md:24`, `u1-.../functional-design/entities.md:95` — explicitly sanctioning a **crate-boundary exception into oxidean-api**, which directly contradicts entities.md:3 "No logic, no route code — rows + signatures only").
  - `crates/oxidean-api/src/routes/feeds.rs:199` `short_ref(&row.ref_name)`, `:200` `let n = row.commits_count` then `n <= 0` comparisons, `:235` `format!("updated {} ({other})", row.ref_name)` (Option has no Display), `:250-251` `row.before_oid.as_str()`/`after_oid.as_str()` (no `as_str` on Option), `:268` `&row.ref_name`, plus test literal `RepoActivityRow{…}` at `:546` and `rename.ref_name = ...` at `:571`. **feeds.rs is not listed in any u1/u2 doc.**
  - `crates/oxidean-api/src/admin/users.rs:335-336` — `ref_name: Some(r.ref_name.clone())`, `commits_count: Some(r.commits_count)` (`AdminUserActivityItem` fields already `Option` — `auth_types.rs:558-560` — de-wrap only).
- **Fix:** Add to scope: widen `RepoActivityRow` 4 fields → `Option`, `map_activity!` → `try_get::<Option<…>>` (keep the i32→i64 fallback for `commits_count`, `repo_activity.rs:33-36`), `InsertRepoActivity` struct (`repo_activity.rs:237-248`) → `Option`, and de-wrap/adapt the three consumer sites above. Update entities.md:3 scope line to name the oxidean-api exception.

**2. BLOCKER — SQLite `repository_activity` alteration requires a full table rebuild; artifacts say "DROP the CHECK / → NULL"**

- **Artifact:** entities.md:116-120, BR2.5 (rules.md:31), Q3 — "drops the `push_type` CHECK and makes the four columns NULL". SQLite has no `ALTER … DROP CONSTRAINT` and no `DROP NOT NULL` — only rebuild works.
- **Pin:** exact precedent exists — `migrations/sqlite/0025_repo_activity_branch_rename.sql:3-30` (`PRAGMA foreign_keys = OFF` → `CREATE TABLE repository_activity_new` → copy → `DROP` → `RENAME` → recreate `idx_repository_activity_repo_created` + `idx_repository_activity_repo_type_created` → `PRAGMA … ON`). Note also `PRAGMA foreign_keys` is a no-op inside a transaction; the 0025 rebuild happens to work regardless, so copy the idiom verbatim.
- **Other dialects (also unpinned):** postgres — `DROP CONSTRAINT repository_activity_push_type_check` (named, added at `postgres/0025:6-10`) + `ALTER COLUMN … DROP NOT NULL` ×4; mysql — `DROP CHECK chk_repository_activity_push_type` + `MODIFY COLUMN … NULL` (precedent `mysql/0032:11-25`). Keep/drop the `DEFAULT ''`/`DEFAULT 0` — either is correct since inserts bind explicit NULLs.
- **Fix:** Pin the three per-dialect alteration recipes in entities.md/rules.md, with the sqlite rebuild + index recreation called out.

**3. BLOCKER — `key` is a reserved word on MySQL; `DerivedIndex` PK `(repo_id,kind,key)` fails CREATE TABLE**

- **Artifact:** entities.md:80-88 (`key` attribute, `PK (repo_id,kind,key)`), BR2.9, functional-spec — column literally named `key`.
- **Pin:** MySQL 8.0 reserves `KEY` (dev.mysql.com keywords list, (R)); every existing column is `key_id`/`key_type`/`token_hash` — no bare `key` and **no backtick quoting anywhere in the 156 migrations**. Postgres: unreserved (ok); SQLite: accepted unquoted in practice.
- **Fix:** Rename to `index_key`/`entry_key` across all three dialects (house naming style) — or pin `` `key` `` quoting on mysql DDL *and* every query, which diverges from house style. Rename is cleaner.

**4. BLOCKER — `object_link.insert` as plain INSERT makes second changesets sharing a blob uncommittable**

- **Artifact:** entities.md:107 `object_link: {insert, count_for_repo, live_hashes}`, WF2 step 3 "INSERT object_links".
- **Pin:** PK is `(repo_id,hash)`; any later changeset in the same repo re-referencing an already-linked blob (revert, merge, untouched-file-in-manifest) collides on the PK → `create_tx` rolls back entirely (BR2.6 makes the whole commit fail).
- **Fix:** Pin insert-if-absent semantics: pg `ON CONFLICT DO NOTHING`, sqlite `INSERT OR IGNORE`/`ON CONFLICT…DO NOTHING`, mysql `INSERT IGNORE` (precedents: `lfs.rs:147-167`, `stars.rs:19,49,78`).

**5. BLOCKER — "TEXT" type pins are physically impossible on MySQL for PK/FK columns; length caps are undecided schema contract**

- **Artifact:** entities.md throughout — `repo_id "TEXT FK repositories(id)"`, `hash "TEXT PK"`, `path "TEXT NOT NULL"` inside `PK (changeset_id,path)`, `kind`/`key`/`name`/`git_oid`/`idem_key` all "TEXT" inside PK/UNIQUE.
- **Pin:** InnoDB cannot put a PRIMARY KEY on a TEXT column (no prefix indexes in PKs), FK types must match `repositories.id = CHAR(36)` (`mysql/0007:4`), and utf8mb4 composite PKs have a 3072-byte budget — `(changeset_id CHAR(36)=144B + path VARCHAR(1024)=4096B)` **exceeds it** (house path precedent is non-PK `VARCHAR(1024)`/`512`, `mysql/0016:57`, `0021:42`). Also mysql TEXT cannot carry DEFAULT (`mysql/0007:2` comment). Correct idioms exist: `CHAR(36)` ids, `CHAR(64)` hashes (`mysql/0012:5`), `VARCHAR(n)` bounded strings.
- **Fix:** entities.md needs per-dialect type pins: pg/sqlite TEXT; mysql `CHAR(36)` (repo/changeset ids), `CHAR(64)`/`VARCHAR` (hashes — see #12), `VARCHAR(n)` with n chosen to fit the 3072 budget for `path`/`key`/`name`/`kind`/`idem_key`.

**6. MAJOR — `parents "TEXT[] / JSON array"` and `payload "BLOB/JSONB"` contradict the house JSON convention**

- **Artifact:** entities.md:44, 85.
- **Pin:** zero `TEXT[]`/`JSONB`/`ARRAY` in all 156 migrations; JSON is stored as `TEXT` on **all three** dialects with a `String` row field and `serde_json` at the boundary — `0047_pat_fg_perms.sql` (`perms_json TEXT NULL` ×3, `PatRow.perms_json: Option<String>`, parsed at `pat/mod.rs:95-97`), `webhooks.rs:14` (`events: String`, `serde_json::to_string` at `webhook/mod.rs:171`). A `Vec<String>` field cannot bind uniformly (sqlx binds `Vec<String>` to TEXT[] on pg only).
- **Fix:** Pin `parents TEXT` (JSON array) and `payload TEXT` on all three dialects; row fields `String` (parse at the oxidean-vcs/core boundary). If payload is genuinely binary, pin `BYTEA/BLOB/BLOB` + `Vec<u8>` — but TEXT-JSON matches precedent.

**7. MAJOR — `vcs.status` is unservable from the C5 surface: no `changeset_count`, no byte-sum, ambiguous `count_for_repo`**

- **Artifact:** entities.md:107 (`count_for_repo`), C5 (contract-summary.md:181).
- **Pin:** C1 `vcs.status` out = `{enabled, heads, changeset_count, object_count, storage_bytes}` (contract-summary.md:42). `count_for_repo` — return type unpinned — plausibly yields `object_count` (`COUNT(object_links)`), but `storage_bytes` requires `SUM(vcs_objects.size) JOIN object_links` (precedent `lfs.rs:398-440` `repo_logical_bytes`), and **nothing counts changesets per repo at all** (`vcs_changesets` has no count method).
- **Fix:** Pin `count_for_repo -> {object_count: i64, storage_bytes: i64}` (one joined query) and add `changeset.count_for_repo -> i64` — or explicitly document that u7 must get these another way. Flag upstream to contract-design since C5 is "frozen".

**8. MAJOR — `cas_update` return shape unpinned; `vcs.ref_conflict{data.actual}` cannot travel through `Result<_, String>`; `RETURNING`/`IS NOT DISTINCT FROM` are pg/sqlite-only**

- **Artifact:** rules.md:43 (BR2.7), functional-spec.md:35-37 (WF3), error table rules.md:71.
- **Pin:** every db function returns `Result<T, String>` — `Err(String)` can't carry `actual`. Contract C1 requires `vcs.ref_conflict` `data:{actual}` (contract-summary.md:28). Also: MySQL has no `RETURNING` (use `.rows_affected()` — `stars.rs:25-26,55-56` — then SELECT actual) and no `IS NOT DISTINCT FROM` (needs `<=>`); sqlite supports `IS NOT DISTINCT FROM` (3.39+). `expected_old=NULL` + absent row → the UPDATE-then-INSERT create path races; pg/sqlite can use `INSERT … ON CONFLICT … DO UPDATE … WHERE` (upsert WHERE is supported on both), mysql needs UPDATE→INSERT in the `create_tx`-style tx. `ref.delete` vs `cas_update(new=NULL)` overlap unspecified (delete = row DELETE presumably — AC5.2.1 wants absent refs, not NULL tombstones).
- **Fix:** Pin `cas_update(repo,name,expected_old,new) -> Result<CasOutcome,String>` where `CasOutcome = Applied(RefRow) | Conflict{actual: Option<String>}`; specify per-dialect null-safe equality (`IS NOT DISTINCT FROM` / `<=>`), rows-affected detection, and delete = `DELETE` row semantics.

**9. MAJOR — zero secondary indexes specified; GC queries and cascade deletes will seq-scan**

- **Artifact:** entities.md — only PKs/UNIQUE constraints listed.
- **Pin:** (a) `list_orphaned_before` is a `LEFT JOIN … IS NULL` anti-join on `object_links(hash)` — house precedent *does* index the link FK side: `idx_lfs_object_links_oid` (`postgres/0012:18`, `mysql/0012:20`). Without `INDEX vcs_object_links(hash)` the GC scan is O(objects × links). (b) `vcs_objects(created_at)` for the grace-bound scan (LFS precedent skipped it — acceptable but worth deciding). (c) `vcs_changesets(repo_id)` — repo_id is a non-prefix FK column; repo-delete cascade + any repo-scoped changeset query seq-scans without it (house indexes FK cols: `idx_repositories_owner_id`, `idx_repository_activity_repo_created`). (d) FK-side indexes for cascade checks: `vcs_refs(changeset_id)`, `vcs_file_changes(blob_hash)`, `vcs_import_links(changeset_id)`, `vcs_derived_index(covering_changeset)`.
- **Fix:** Add an `indexes:` block to entities.md — minimum: `object_links(hash)`, `changesets(repo_id)`; recommended: `objects(created_at)` + the three FK-side indexes.

**10. MAJOR — BR2.8 idempotency race has no pinned mechanism; error-string matching differs per dialect**

- **Artifact:** rules.md:48-51, traceability AC2.2.2 ("UNIQUE race ⇒ one row; loser reads existing").
- **Pin:** unique-violation text differs — pg "duplicate key value violates unique constraint", mysql "Duplicate entry … for key", sqlite "UNIQUE constraint failed". `Result<_, String>` forces brittle substring matching unless the idiom avoids the error: `INSERT … ON CONFLICT DO NOTHING`/`INSERT OR IGNORE`/`ON DUPLICATE KEY UPDATE idem_key=idem_key` + `rows_affected()==0` → re-SELECT (exact `star_repository` pattern, `stars.rs:16-44`). Also WF2 orders `find_by_idempotency` *before* `BEGIN` — the lookup must be inside or retried-after-conflict to close the window.
- **Fix:** Pin the insert-ignore-then-reselect idiom (or run the idem lookup + insert inside the same tx with serializable semantics) instead of error-string detection.

**11. MINOR — stale CHECK literal in entities.md:118**

Pins `push_type IN ('push','force_push','pr_merge','branch_creation','branch_deletion')` — migration `0025` (×3) already added `'branch_rename'` (live CHECK at `postgres/0025:7-10`, `sqlite/0025:9-11`, `mysql/0025:6-10`). Cosmetic, but the artifact quotes the constraint verbatim — fix the literal.

**12. MINOR — unpinned details an implementer will guess on**

- `vcs_changesets(id)` FK target (entities.md:58,73,86,97) vs the entity's own PK field named `changeset_id` (entities.md:42) — pick `id` (house convention: `repository_activity.id`, `lfs_objects.oid`) or rename consistently.
- Stored form of `hash`: entities.md:22 says "hex object id" (bare 64) but the wire returns `sha256:…` (C2, contract-summary.md:137; `vcs_types.rs:169` validates `sha256:` + 64 hex). Mysql `CHAR(64)` vs `CHAR(71)`/`VARCHAR(71)` depends on this — pin it.
- `changeset_id` format is `cs_` + 26-char ULID (`vcs_types.rs:154-167`) → `CHAR(29)`/`VARCHAR(32)` on mysql; `git_oid` is 40-hex.
- `create_tx`/`insert_if_absent`/`parents_of` return types unpinned — `insert_if_absent` should return `bool`/inserted-vs-dedup (u3's `Stored|Dedup`, contract-summary.md:194; `rows_affected` idiom covers it). `parents_of` vs `get` redundancy — `parents` is a column on the same row.
- `committed_at` is caller-supplied `TIMESTAMP NOT NULL` with no default — binds as `&str`; pg needs `$n::timestamptz` cast at bind site (precedent `repo_activity.rs:336`), mysql/sqlite take the ISO text — worth one line.
- `ref.name` stored form (bare `main` vs `refs/heads/main`) unpinned — determines CAS key equality.
- `file_changes.blob_hash` FK + link-based liveness: correctness relies on the unstated invariant "every manifest blob has an object_link in the same repo" (u3/u4's job) — document it so `list_orphaned_before` isn't misread; the FK itself is then a protective assertion (object delete while a `file_change` references it = FK violation, fail-safe).
- `VcsPersistenceFacade` is listed as a 9th entity under "eight new row entities" (entities.md:6-7,102-113) — cosmetic.

**Verified OK (no action):** façade-by-value (`Database: Clone`, `lib.rs:110-114`); `impl Database` in `vcs.rs` can call private `require_pool` (same-crate visibility); `skipped()`→`db.not_configured` convention real; per-dialect `p.begin()`+`execute(&mut *tx)`+`commit` tx idiom exists for `create_tx`; `AssertSqlSafe` precedent for dynamic SQL (`repo_activity.rs:193`); upsert triplets (`lfs.rs`, `watches.rs`, `packages.rs`); `INSERT IGNORE`/`INSERT OR IGNORE`/`ON CONFLICT` for dedup; timestamp conventions `TIMESTAMPTZ`/`DATETIME(6)`/`TEXT`+`strftime`; `push_type` stays NOT NULL (only CHECK drops); `actor_id`/`id` stay non-null.

**Actions the parent should handle:** none blocked — all findings are artifact edits. If severities must be triaged: 1–5 are compile/migration-failure certain; 6–9 are design corrections that are cheap now and painful post-codegen; 10–12 are clarifications.
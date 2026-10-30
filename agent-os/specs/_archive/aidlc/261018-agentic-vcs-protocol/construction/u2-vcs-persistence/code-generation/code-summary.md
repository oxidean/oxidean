# Code Summary — u2-vcs-persistence

## Landed

| File | Change |
|------|--------|
| `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0053_vcs_schema.sql` | 8 VCS tables + 7 secondary indexes + `repository_activity` nullable-field alteration (per-dialect: pg DROP CONSTRAINT/ALTER COLUMN, mysql DROP CHECK/MODIFY, sqlite full rebuild + index recreation per 0025 precedent) |
| `crates/oxidean-db/src/vcs.rs` (new, ~3200 loc) | 8 row structs, `CasOutcome`, `VcsRepoObjectCounts`, insert structs, 21 `vcs_*` façade methods, 15 sqlite unit tests |
| `crates/oxidean-db/src/repo_activity.rs` | `RepoActivityRow` + `InsertRepoActivity` fields → `Option`; `map_activity` decodes `Option` |
| `crates/oxidean-db/src/lib.rs` | `pub mod vcs`, re-exports, `insert_repo_activity` signature widened |
| `crates/oxidean-api/src/repo/activity.rs` | `row_to_item` passthrough; 5 insert sites → `Some(...)` |
| `crates/oxidean-api/src/routes/feeds.rs` | `activity_title`/`activity_link` Option-safe (`""`/`0` fallbacks); test literals |
| `crates/oxidean-api/src/admin/users.rs` | passthrough fields |
| `crates/oxidean-api/tests/{repo_activity_list,atom_feeds}.rs` | `Some(...)` call sites |
| `aidlc/.../contract-design/contract-summary.md` | C5 yaml amended (insert_if_absent, index_key, count_for_repo, CasOutcome) + errata |

## Pinned decisions implemented

- **FK actions**: repo_id→CASCADE everywhere; changeset-pointing FKs→CASCADE; `object_links.hash`/`file_changes.blob_hash`→RESTRICT/NO ACTION (GC protection); `enabled_by`→SET NULL.
- **CAS**: `vcs_cas_ref_update` → `CasOutcome{Applied|Conflict{actual}}`; null-safe equality (`IS NOT DISTINCT FROM` pg/sqlite, `<=>` mysql); `new=NULL` = delete only; mysql uses `rows_affected`+reselect (no RETURNING).
- **create_tx**: single tx — pre-validates parents + blob hashes inside tx with sentinel errors `vcs.not_found:parent`/`vcs.not_found:object` (typed for u7 mapping, no substring sniffing); object links insert-if-absent; idem reselect for dedup.
- **Idempotency**: insert-if-absent on object_links + import_links — no error-string matching.
- **Encodings**: `parents`/`payload` TEXT-JSON; bare hex hash/git_oid (CHAR(64) — sha256-format safe); `index_key` (not `key`); changeset PK `id` (CHAR(29) pg / VARCHAR(32) mysql / TEXT sqlite, FK-consistent).
- **Pagination**: `clamp(1,500)` on list/query/page methods.
- **Rollback truth**: applied migration files are never deleted — revert is code-only or compensating forward migration (sqlx `VersionNotPresent` would brick boot).

## Verification

- `cargo nextest run -p oxidean-db -p oxidean-api` — **930 passed, 3 skipped**
- `cargo clippy -p oxidean-db --all-targets -- -D warnings` — clean
- `cargo fmt --check` — clean (one pre-existing assert reflow in rpc.rs)
- `make rpc-sync-check` — ok, zero diff

## Known carry-forwards

- pg/mysql migrations unexercised until `db-dialects`/`compose-smoke` lanes (sqlite-only unit tests).
- GC interplay note for u3: `list_orphaned_before` anti-joins links; RESTRICT on `file_changes.blob_hash` means link-orphaned objects may still be manifest-referenced — deletion will fail at FK, intentional.

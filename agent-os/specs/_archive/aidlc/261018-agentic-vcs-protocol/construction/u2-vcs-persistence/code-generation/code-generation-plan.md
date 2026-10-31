# Code Generation Plan — u2-vcs-persistence

## Deliverables

1. **`crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0053_vcs_schema.sql`** — 8 tables + indexes + repository_activity alteration (per-dialect recipes).
2. **`crates/oxidean-db/src/vcs.rs`** — row structs + `CasOutcome` + `impl Database` façade (~20 methods).
3. **`crates/oxidean-db/src/repo_activity.rs`** — RepoActivityRow/InsertRepoActivity → Option; map_activity → try_get::<Option>.
4. **`crates/oxidean-db/src/lib.rs`** — `pub mod vcs`, re-exports, insert_repo_activity signature.
5. **API de-wraps** — `repo/activity.rs` row_to_item; `routes/feeds.rs` (~8 sites+2 literals, "" / 0 fallbacks); `admin/users.rs` pass-through; insert call sites Some(...) (activity.rs:144,183,220,264,299 + tests repo_activity_list.rs:181, atom_feeds.rs:245,422,436).
6. **Unit tests** in vcs.rs (sqlite): skipped-DB fail, flag get/upsert, create_tx atomicity, idempotent resubmit, CAS Applied/Conflict{actual}, insert-if-absent on links, cascade deletes, pagination clamps.

## Pinned corrections (review findings — MUST implement)

- **FK actions**: repo_id→CASCADE; file_changes/refs/derived_index(covering)/import_links .changeset_id→CASCADE; object_links.hash + file_changes.blob_hash→RESTRICT/NO ACTION; enabled_by→SET NULL.
- **FK-error mechanism (FD R-13)**: inside create_tx pre-validate parents exist (SELECT id FROM vcs_changesets WHERE id IN parents) + blob hashes exist — typed sentinel errors `"vcs.not_found:parent"`/`"vcs.not_found:object"` (exact-match strings, house convention) so u4/u7 map without substring sniffing. Do NOT rely on raw FK errors.
- **import_link insert** = insert-if-absent.
- **git_oid** = CHAR(64)/TEXT (covers sha256-format repos).
- **cas_update**: `new=NULL` ⇒ delete path only (never inserts NULL-changeset row). `CasOutcome{Applied(VcsRefRow)|Conflict{actual:Option<String>}}`.
- **object_link** = insert-if-absent inside create_tx.
- **key column** = `index_key`.
- **parents/payload** = TEXT JSON; hash/git_oid bare hex; ref.name bare; changeset PK col `id` CHAR(29)/VARCHAR(32).
- **pagination clamps** `clamp(1,500)` on ref.list, derived_index.query, import_link.cursor_page.
- **Secondary indexes**: object_links(hash), changesets(repo_id), objects(created_at), refs(changeset_id), file_changes(blob_hash), import_links(changeset_id), derived_index(covering_changeset).
- **activity alteration** per-dialect: pg DROP CONSTRAINT repository_activity_push_type_check + ALTER COLUMN ×4 DROP NOT NULL; mysql DROP CHECK chk_repository_activity_push_type + MODIFY COLUMN ×4 NULL; sqlite rebuild per 0025 + both indexes.
- **Rollback truth**: never delete an applied migration file (sqlx VersionNotPresent→exit(1)); revert = code-only or forward migration.
- **C5 yaml**: amend contract-summary.md block on landing (insert_if_absent, index_key, count_for_repo pair, CasOutcome).

## Verification gates (order)

1. `cargo test --no-run -p oxidean-db -p oxidean-api` (compile sweep)
2. `cargo nextest run -p oxidean-db -p oxidean-api` (tests green incl. existing activity/feeds)
3. `cargo clippy -p oxidean-db --all-targets -- -D warnings`
4. `cargo fmt --check`
5. `make rpc-sync-check` (regen-clean — no wire changes in this unit)

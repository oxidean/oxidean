# Unit Test Instructions — u2-vcs-persistence

## Coverage (all in `crates/oxidean-db/src/vcs.rs` `#[cfg(test)]`, sqlite tempdir)

| Test | Asserts |
|------|---------|
| `vcs_skipped_db_fails_closed` | façade methods on a skipped-DB `Database` return `Err` — no pool dereference |
| `flag_get_upsert_roundtrip` | surface flag absent→None; upsert persists; second upsert overwrites `enabled_by` |
| `create_tx_atomicity` | changeset + file_changes + object_links commit atomically; parents JSON round-trips |
| `idem_resubmit_dedups` | identical idem_key resubmit returns the same changeset row (no insert) |
| `create_tx_aborts_missing_parent` | parent id not in vcs_changesets → `Err` sentinel `vcs.not_found:parent`, tx rolled back |
| `create_tx_aborts_missing_blob` | non-delete change referencing absent blob_hash → `vcs.not_found:object`, rolled back |
| `delete_change_rejects_null_blob_ok` | delete kind with NULL blob accepted; non-delete NULL blob rejected (shape violation) |
| `cas_applied_and_conflict` | `cas_update` on absent→create=Applied; stale expected→Conflict{actual=Some}; delete path Applied |
| `object_link_insert_if_absent` | duplicate link insert dedups (rows_affected 0, no error) |
| `import_link_insert_if_absent` | duplicate git_oid→changeset link dedups; lookup returns link |
| `repo_delete_cascades` | deleting repo row cascades refs/changesets/links/derived_index; global vcs_objects survive |
| `derived_index_upsert_merge` | upsert merges (repo,kind,index_key); covering_changeset updated; query paginates |
| `pagination_clamps` | ref.list/derived_index.query/import_link.page honor limit clamp(1,500) + cursor |
| `ref_free_activity_row` | insert_repo_activity with all-None fields round-trips; map_activity decodes Option |
| `repo_object_counts` | `{object_count, storage_bytes}` aggregates join objects↔links |

## Pre-existing coverage exercised

- `repo_activity_list.rs`, `atom_feeds.rs` — widened `Option` fields + `Some(...)` call sites compile and pass under the sqlite fixture.

## Gaps carried forward

- pg/mysql migrations are apply-only-probed by the `db-dialects`/`compose-smoke` lanes; façade behavior tests are sqlite-only (house precedent).
- GC interplay (`list_orphaned_before` vs `file_changes.blob_hash` RESTRICT) deferred to u3 object-store unit.

# Tech-Stack Decisions — u5-vcs-merge-engine

## TD5.1 — Zero new dependencies (verified against Cargo.toml)

The `server` feature already carries everything `merge_engine.rs` needs: `bytes` (put-stream items), `futures-util` (`stream::once`), `tokio` (`spawn_blocking`; `io-util` for `AsyncReadExt::take` on the `tokio::fs::File` that `ObjectStore::get` returns — devsecops 11 correction), `oxidean-core` (`ChangesetId`, `VcsConflictEntry`), `oxidean-db` (`Database` facade), `sha2`, `chrono`, `tracing`, `serde`/`serde_json` (unused — no new serializable shapes; kept only because u4 already claims them). **No Cargo.toml edits** — the `client` feature stays dependency-free; `merge_engine` is `#[cfg(feature = "server")]`-gated beside `changeset_store`/`read_index`/`object_store`.

## TD5.2 — Module layout + consumed internals

One new file `crates/oxidean-vcs/src/merge_engine.rs` (server-gated, re-exported from `lib.rs`'s server block). Consumed crate-internal items (all existing, no moves): `read_index::{ReadIndex, TreeEntry, split_lines (pub(crate)), IndexError}`, `text_diff::{diff_lines, LINE_CAP}`, `changeset_store::{ChangesetStore, CreateChangesetInput, FileChangeInput, ChangeKind, ChangesetError}`, `object_store::{ObjectStore, ObjectStoreError, QuotaScope}`, `oxidean_core::vcs_types::{ChangesetId, VcsConflictEntry}` (devsecops 12 — `VcsFileChangeInput` does not exist; the wire-side manifest type is `ManifestEntry` and the store-side inputs are `FileChangeInput`/`ChangeKind`). `MERGE_PAGE: usize = 500` is a private u5 const — u4's `SCAN_PAGE` is private and stays so (no read_index.rs edit; u4 files are fingerprint-bound).

## TD5.3 — Time + determinism seams

`committed_at` never set by the engine — `CreateChangesetInput.committed_at: None` → `Utc::now()` inside `create` (TD4.6), excluded from `canonical_input` so ids stay content-derived and resubmission lands on `idem_key`. `merge`/`rebase` take `author_name`/`author_email` params threaded into `create` — **validated TWICE by design**: a mirrored deterministic pre-check at engine entry/WF1.9 (so caller-input `InvalidInput` can't fire post-`put` — BR5.16's "no re-check" is STRUCK, reviewer-3 F3) and authoritatively inside `create`. Default merge message `format!("merge {b} into {a}")` with `{a,b}` the canonical id strings — deterministic.

## TD5.4 — Facade + store call discipline (zero new surface)

Only the pinned consumed surface: `db.vcs_get_changeset` (row probes), `db.vcs_changeset_parents` (BFS walks), `db.vcs_list_file_changes(changeset_id)` (provenance manifests — repo-less signature verified `vcs.rs:2475`), `index.merge_base` + `index.tree(repo, cs, prefix: None, limit, cursor)` (u4 page shape — cursor PARAM is `Option<&str>`; the page's returned cursor is `Option<String>` — pass `.as_deref()` on the loop, devsecops 13), `objects.get` + `objects.put(stream, repo_id)` with `stream::once(async move { Ok::<Bytes, Infallible>(bytes) })` (verified `object_store/mod.rs:196-202`), `store.create`. **Zero new facade methods, zero new index kinds, zero u4/u3/u2 file edits** (BR5.19).

## TD5.5 — Test posture (NFR10 — same gate as TD4.8)

`cargo nextest run -p oxidean-vcs --features server` + `cargo clippy -p oxidean-vcs --all-targets --all-features` — zero-test runs are failures. sqlite-backed integration coverage: fast-forward both directions; equal heads; both-add-distinct union (two-parent changeset, manifest vs first parent); text merge success (clean diff3 → merged blob stored + modify row); content conflict (overlapping edits); binary conflict (NUL byte); type conflict (symlink-vs-file); mode conflict (chmod-vs-edit — BR5.24); delete-vs-modify; rename/rename (both rename same old to different targets); rename-vs-delete; chained-rename provenance (a→b→c emits rename a→c); ambiguous-provenance drop; trailing-newline-only divergence conflicts; merge3 byte-faithful round-trip; rebase linear replay; rebase of a merge node (extra parents dropped); rebase no-op (`new_parent == old_base` → same id); disjoint rebase target legal; descendant target → `InvalidInput`; root rebase (∅ base); idempotent resubmit same id; `TooLarge` via injected `MergeCaps` (paths/blob/walk/rename) + a REAL >`LINE_CAP`-line blob fixture for the line cap (LINE_CAP isn't a caps field — devsecops 15); `QuotaExceeded` scope passthrough; zero rows AND zero object links on conflict (probe `vcs_get_changeset`/`vcs_list_file_changes`/`vcs_get_object_link` after abort — devsecops 1's link leak must stay closed); `DbNotConfigured` variant surfaced (SR5.9). `merge3` unit tests on owned vectors (no DB).

## TD5.6 — Error channel

One enum `MergeError` per entities.md — no `PartialEq` (recorded deviation, SR5.11). `From` impls at the consumption sites only (no blanket impls): `IndexError`→per Q8 map, `ChangesetError`→per Q8, `ObjectStoreError`→per Q8, `io::Error`→`Internal` (blob reads), facade `String`→`Internal`. Wire mapping table pinned in WF6 — u7 consumes variants, never strings.

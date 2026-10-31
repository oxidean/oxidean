# Developer Support Contribution — u3-vcs-object-store functional-design

Blind-pass review of `aidlc/spaces/default/intents/261018-agentic-vcs-protocol/construction/u3-vcs-object-store/functional-design/{functional-design-questions.md,functional-spec.md,entities.md,rules.md,traceability.json}` against real source. Note: `u4-vcs-changeset-core/` has **no artifacts yet** (nothing to cross-check the C6a consumer side against).

## Findings

**1. BLOCKER — GC's "delete row" step has no facade function; there is no `DELETE FROM vcs_objects` anywhere**
- **Where:** functional-spec.md WF4.2 ("`list_orphaned_before` → delete row + file"), rules.md BR3.6 ("orphan-row pass (list_orphaned_before → delete row then file)"), entities.md:26 ("deleted only by GC pass 2").
- **Evidence:** `crates/oxidean-db/src/vcs.rs` exposes `vcs_get_object`, `vcs_insert_object_if_absent`, `vcs_list_orphaned_objects_before`, `vcs_insert_object_link_if_absent`, `vcs_repo_object_counts`, `vcs_live_object_hashes`, `vcs_create_changeset_tx`, etc. (facade impl at lines 2269–2468) — **no delete for `vcs_objects`**. Repo-wide grep for `DELETE FROM vcs_objects` / `vcs_delete_object` / `DELETE FROM vcs_object_links` returns zero hits. The LFS precedent the design leans on has the missing piece: `db.delete_lfs_object` (`crates/oxidean-db/src/lib.rs:4146`, `lfs.rs:576`). `vcs.rs:535–538` even says "Rows only: deleting one is blocked by `object_links`/`file_changes` RESTRICT FKs" — the scan exists, the delete was never built.
- **Fix:** needs a U2 delta — `vcs_delete_object(hash)` (ideally conditional `DELETE … WHERE hash=$1 AND NOT EXISTS (link)` to shrink the race window). Cannot be worked around from U3; dialect SQL may not live outside `oxidean-db`.

**2. BLOCKER — `insert_links_tx(repo_id, [hashes])` as a U3-owned callable cannot exist; the real mechanism is the `object_links` parameter of `vcs_create_changeset_tx`**
- **Where:** functional-spec.md:17 (mermaid `insert_links_tx inside create_tx`), WF3 (line 34–35); traceability.json C6a; frozen contract at `inception/contract-design/contract-summary.md:196` (`object_store (u3 → u4): "… insert_links_tx(repo_id, [hash])"`, owner u3, line 16).
- **Evidence:** `create_changeset_tx(pool, changeset, file_changes, object_links: &[&str], index_upserts)` (vcs.rs:829–835, facade wrapper 2329–2344) does `p.begin()` internally and `tx.commit()` internally — **no transaction handle ever crosses the facade boundary**, and `DbPool`/`sqlx::Transaction` are not an injectable executor. A standalone U3 `insert_links_tx` on the pool would commit links *outside* the changeset tx — breaking the C6a atomicity claim and leaking permanent links on commit failure (no `delete_object_link` exists; links die only by repo CASCADE).
- **The good news:** the intended semantics already exist in the facade — in-tx hash validation (`SELECT hash FROM vcs_objects WHERE hash IN …` → `ERR_NO_OBJECT = "vcs.not_found:object"`, vcs.rs:21, 875–890) and in-tx `INSERT … ON CONFLICT DO NOTHING` links (vcs.rs:940–950).
- **Fix:** reword WF3/C6a — U3 ships no such function; U4 passes every referenced blob hash via `object_links` in `vcs_create_changeset_tx`. Also pin the invariant U4 must satisfy: `create_preflight` (vcs.rs:742–751) unions `file_changes.blob_hash` + `object_links` for the *existence* check but only inserts links for `object_links` entries — if U4 passes only "new" blobs, referenced-but-unlinked objects become "orphaned" rows whose GC delete then FK-aborts on `vcs_file_changes.blob_hash` RESTRICT (migrations/postgres/0053:52) — permanent error noise (u2 nfr-review R-03, still Unresolved; also flagged in u2's own dev contribution item 12).

**3. MAJOR — "skip quota check when the (repo,hash) link already exists" has no cheap membership probe**
- **Where:** functional-spec.md WF1.3, rules.md BR3.4(b), functional-design-questions.md Q3.
- **Evidence:** the facade has no `link_exists`/`get_object_link`. Workable orderings all hurt:
  - `vcs_live_object_hashes(repo_id)` → `.contains(hash)`: correct but fetches **every** linked hash of the repo per PUT (vcs.rs:698–722).
  - `vcs_insert_object_link_if_absent` first, then quota check: the denied PUT's link is already committed — bytes permanently attributed, object pinned from GC, and there is no `delete_object_link` to compensate.
  - Unconditional `repo_object_counts + size > quota`: double-counts an existing link and wrongly denies re-references in an over-quota repo — exactly what BR3.4's skip exists to prevent.
- **Fix:** ask U2 for `vcs_get_object_link(repo_id, hash) -> bool` (breaks the traceability "no new SQL" claim — flag upstream), or pin the `live_object_hashes` scan as the accepted probe.

**4. MAJOR — the row-less file sweep can delete the live `.gc-lock` and any non-store file**
- **Where:** functional-spec.md WF4.3 ("non-tmp files with no ObjectEntry row AND mtime older than grace → remove"); entities.md:40 places `GcLockFile` at `OXIDEAN_VCS_OBJECTS_DIR/.gc-lock` — **inside the walked tree**.
- **Evidence:** `.gc-lock` satisfies the delete predicate (no row, mtime can exceed grace). A long-running or stale-lock pass can unlink its own lock → a second `gc_once` starts → BR3.6 mutual exclusion defeated. The same predicate deletes any stray file in the dir (operator drop-ins, `lost+found`, non-`.tmp` tmp-dir names) — `get_object("<garbage name>")` returns `None` → "row-less" → deleted.
- **Fix:** pin the sweep predicate: only paths matching `{2-hex}/{2-hex}/{64-hex}` shard shape, explicit exclusion of dotfiles/the lockfile, and mirror `wipe_lfs_dir_contents`' canonicalize-and-refuse-outside-root guard (lfs_gc.rs:106–127) plus no-follow on symlinks.

**5. MAJOR — "refuses only a live lock" has no specified mechanism, and the claimed LFS precedent doesn't contain one**
- **Where:** functional-spec.md WF4.1 ("already-held ⇒ skip"), WF5 ("`gc_once` itself refuses only a live lock"), Q4 ("stale-PID semantics, LFS precedent").
- **Evidence:** LFS `try_acquire_lock` (lfs_gc.rs:84–99) is `create_new(true)` → `AlreadyExists` → skip, writes a pid, deletes the file on success — **the pid is never read back and no liveness check exists anywhere**. So "stale-PID semantics, LFS precedent" is factually wrong; this is new mechanism, not precedent. A mid-uptime GC crash wedges all passes until next boot unless `gc_once` checks holder liveness — and the design never says how (pid + `kill(0)`/`/proc` read? lock age? `flock`?). Each implies different deps/platform caveats.
- **Fix:** pin the mechanism, or explicitly accept "any lockfile ⇒ skip + boot clear only" and record the mid-uptime wedge as a limitation.

**6. MINOR — Q2's tmp path is literally impossible; artifacts disagree**
- functional-design-questions.md:10: "Stream → `{dir}/{hash-ab}/{hash-cd}/{full-hash}.tmp` while hashing" — the digest isn't known until the stream ends. WF1.1 correctly says `{tmp-dir or shard}/{uuid}.tmp`. Align Q2 with WF1. (Side note: LFS's `final_path.with_extension("tmp")` at store.rs:53 shares one tmp path across concurrent same-oid PUTs — U3's uuid-tmp avoids that race; tmp-name minting needs no new dep: pid+counter+nanos works.)

**7. MINOR — dep list under-specified; `put(reader)` type unpinned**
- Q1 lists `oxidean-db`/`sha2`/`tokio` (+ shared `oxidean-core`) but the workflows also need: `futures-util`+`bytes` (the codebase's stream shape is `S: StreamExt<Item = Result<Bytes, E>> + Unpin` — store.rs:35–43; contract "reader" is ambiguous, do **not** implement `AsyncRead` without a decision); `tracing` (WF4.4 stats logging); `chrono` or equivalent for the `grace_ts` ISO cutoff (lfs_gc.rs:55–57 idiom — `std::time` cannot format dates); `tokio-util` if `get()` returns `ReaderStream` (LFS `read_object` buffers `Vec<u8>` — at 256 MiB objects, pin whether U3 returns a stream or a `tokio::fs::File`); `thiserror` (Q6 hedges); a local `bytes_to_hex` (precedent is `oxidean-api::auth::session:275` — unreachable given BR3.1's dependency direction). Feature-gate mechanics themselves verified feasible — see Verifications.

**8. MINOR — `Db(String) → db.not_configured` must be driven by `is_skipped()`, not error-text**
- rules.md:55/73 + Q6 scope `db.not_configured` to the skipped pool, but every facade error is a `String` (`require_pool` → `"database not configured"`, lib.rs:156–160 — not a pinned sentinel; genuine mid-query failures produce different strings and must map to host-internal, not `db.not_configured`). `Database::is_skipped()` exists (lib.rs:131–133) — pin an eager check at `put`/`get`/`gc_once` entry producing a distinct typed variant.

**9. MINOR — GC-vs-re-reference race on old orphans surfaces a non-sentinel FK error**
- Q4/BR3.5 claim "grace + tx ordering" covers PUT→commit races. True for young objects; an object orphaned >grace (e.g., after repo-delete cascades) being re-referenced by a PUT/`create_tx` racing `gc_once` can lose the row mid-check → `vcs_object_links.hash` plain `REFERENCES` FK (postgres/0053:24) → raw sqlx `String` error → host 500, not `vcs.not_found`. Rare and self-healing; pin per-object delete-failure tolerance in `gc_once` and record the edge.

**10. MINOR — `live_hashes` in the traceability "consumed facade" list is per-repo and probably vestigial**
- traceability.json C5 claims consumption of `live_hashes`; `vcs_live_object_hashes(repo_id)` (vcs.rs:2320) requires a `repo_id`, and no repo-enumeration or global variant exists in the facade. The global GC pass doesn't need it (`list_orphaned_objects_before` computes referenced-ness by anti-join). Either drop it or pin its use as the Finding-3 probe.

**11. MINOR — two boundary pins missing**
- (a) GET `NotFound`: row-exists-but-file-gone (or file removed between `get_object` and `open`) yields `io::ErrorKind::NotFound` — must map to `NotFound`, not `Io`/`Internal` under BR3.9; carve-out unpinned. (b) `ObjectId::parse` stores and returns the `sha256:`-prefixed string (vcs_types.rs:237–255) and *rejects* bare hex (test at line 978); `is_object_id` is private. Pin whether `put/get/verify` take bare `&str` re-validated by a local `validate_oid` clone, or `ObjectId` + `strip_prefix`.

**12. MINOR — type/edge pins:** `size`/`storage_bytes` are `i64` vs `u64` quotas (cast pin needed); empty PUT produces the sha256-of-empty object with `size=0` — legal under this shape, unacknowledged; `ObjectStore::new`/`put` should `create_dir_all` the objects dir (LFS creates shard parents, store.rs:47–51) and `gc_once` early-return when the dir is absent (lfs_gc.rs:39–41).

## Verifications (checked out)

- `insert_object_if_absent(pool,hash,size) -> Result<bool>` — `true`=Stored / `false`=Dedup, matching the `Stored|Dedup` discriminator (vcs.rs:501–533; facade comment at 2291 literally names u3's mapping).
- `list_orphaned_objects_before(pool, grace_ts)` semantics = "zero live links AND `created_at < grace`" via `LEFT JOIN … IS NULL` — exactly the design's orphan definition (vcs.rs:539–592); supported indexes `idx_vcs_object_links_hash` + `idx_vcs_objects_created_at` exist (postgres/0053:86–87).
- `repo_object_counts(pool, repo_id) -> {object_count, storage_bytes}` over the links⋈objects join — the design's `count_for_repo` (vcs.rs:638–695).
- `insert_object_link_if_absent` returns bool, PK `(repo_id,hash)` — re-reference collisions impossible (vcs.rs:598–634).
- `create_changeset_tx` in-tx behavior: parent check → `vcs.not_found:parent`, object check → `vcs.not_found:object`, insert-if-absent links, full rollback on any error (vcs.rs:836–973) — the C6a *semantics* are real (Finding 2 is about API shape, not capability).
- FK actions: `vcs_object_links.repo_id` → CASCADE, `.hash` and `file_changes.blob_hash` → plain `REFERENCES` (NO ACTION = RESTRICT-equivalent) — matches design claims (postgres/0053:22–27, 46–54).
- `Database::skipped()`/`require_pool`/`is_skipped()` pathway is real (lib.rs:123–160).
- Five `OXIDEAN_VCS_*` names/defaults match the frozen contract table verbatim (contract-summary.md:227–231 vs entities.md:53–57).
- `ObjectId` wire type validates `sha256:`+64-lower-hex — consistent with the "prefix at boundary only" pin.
- Feature-gate feasibility: `oxidean-vcs` skeleton is bare (`Cargo.toml` = `[dependencies]` only, `lib.rs` 6 lines). No crate in the workspace uses `[features]`/`optional` yet (grep) — new pattern but standard cargo: `default = ["client"]`, `server = ["dep:oxidean-db",…]`, `tokio = { workspace = true, optional = true }` are all legal under `resolver = "2"`; `cargo check -p oxidean-vcs` with default features links no `oxidean-db` as BR3.1 requires.
- `validate_oid`/`shard_path`/`put_stream` LFS precedent confirmed (store.rs:13–93).
- U4 doesn't exist yet — `construction/u4-vcs-changeset-core/` has zero files; the consumer side of C6a is unverifiable beyond the facade signature.

## Suggestions

1. Route a change request to U2 for `vcs_delete_object` (prefer link-guarded delete); GC cannot be coded without it.
2. Fix the C6a contract text: the deliverable is "`object_links` param of `vcs_create_changeset_tx` consumed by U4," not a U3 `insert_links_tx`. If a U3 helper is desired for symmetry, make it a pre-commit `assert_objects_exist(hashes)` — explicitly non-atomic documentation, not the mechanism.
3. Decide the quota membership probe: request `vcs_get_object_link` from U2 (small, cheap) vs. the documented `live_object_hashes` scan.
4. Pin the sweep predicate (shard-shaped names only, exclude `.gc-lock`/dotfiles, canonicalize-refuse-outside-root, no-follow symlinks) and the lock-liveness mechanism; correct the "LFS precedent" wording in Q4.
5. Pin `put` arg type to the `StreamExt<Item = Result<Bytes, E>> + Unpin` precedent and enumerate the real dep list (futures-util/bytes, tracing, chrono-or-alternative, tokio-util-or-File, local `bytes_to_hex`).
6. Pin `is_skipped()`-eager `DbNotConfigured`, the `io::ErrorKind::NotFound → NotFound` carve-out in GET, and the per-object delete-failure tolerance (needed for the already-known R-03 RESTRICT-abort case).
7. Pin that U4 must pass *all* manifest `blob_hash`es in `object_links` (else unlinked-but-referenced objects become un-deletable noise for every GC pass).

Actions the parent should handle: none blocked — all findings are artifact edits or upstream change requests to U2/contract-design.
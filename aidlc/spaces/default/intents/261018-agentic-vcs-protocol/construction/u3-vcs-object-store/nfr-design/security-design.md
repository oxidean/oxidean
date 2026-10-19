# Security Design — u3-vcs-object-store

## SD3.1 — Trust boundaries

```
client ──U7 transport/auth──> ObjectStore (trusted inputs: hash strings, byte stream, repo_id)
ObjectStore ──fs──> OXIDEAN_VCS_OBJECTS_DIR (walked + written)
ObjectStore ──facade──> oxidean-db (trusted SQL site)
```
Inside the unit, exactly two untrusted inputs: the hash string (path-join risk) and the byte stream (size/timeout risk). Everything else is operator config (validated at parse) or DB rows (trusted shapes).

## SD3.2 — Path-safety pipeline (SR3.1)

1. `validate_hash(bare &str)` — `len==64 && all lowercase-hex` before ANY `Path::join` (never joins caller text).
2. `shard_path(dir, hash)` = `{dir}/{h[0:2]}/{h[2:4]}/{h}` — identical shape to LFS `store.rs:24`.
3. Sweep walk: `read_dir` entries filtered by `{2hex}/{2hex}/{64hex}` name-shape BEFORE stat; `canonicalize` each candidate and require `starts_with(canonical_root)`; `file_type().is_symlink()` → skip (no-follow).
4. `.gc-lock`, `.oxidean-vcs-objects`, `tmp/`, `lost+found`, dotfiles can never match the shape — excluded by predicate, not a deny-list.

## SD3.3 — PUT safety ordering (SR3.2/BR3.2)

```
stream chunk → hasher.update + count+=len → count>max ⇒ abort(tmp removed, QuotaExceeded{Object})
             → write tmp
finalize → digest → shard path → rename (atomic)
→ insert_object_if_absent (bool → Stored|Dedup)
→ get_object_link(repo,hash)? true ⇒ done
   false ⇒ count_for_repo: storage_bytes+size>quota ⇒ QuotaExceeded{Repo}
           (+ eager cleanup if this PUT Stored: delete_object_if_unlinked — unlink file
             ONLY when it returns true; false = a link raced in, keep the bytes)
           else insert_object_link_if_absent
```
Failure at any point removes `tmp`; post-rename failures leave a globally-valid orphan (grace-GC'd) — never a truncated serve.

## SD3.4 — GC mutual exclusion + liveness (SR3.3/BR3.6)

`create_new(.gc-lock)` acquire → on `AlreadyExists`: read mtime; `now - mtime < gc_grace_secs` ⇒ return `Ok(stats)` with `skipped = true`; older ⇒ remove + retry once. During the pass, touch mtime every N objects via `OpenOptions::new().write(true).open()` + `set_modified(now)` — NOT `File::create` (truncates lock content; cosmetic, but heartbeat must not destroy the file). `LockGuard` removes the file on Drop using sync `std::fs::remove_file` (Drop can't await; covers panic-unwind + task-abort — strictly better than LFS `lfs_gc.rs:79-80`'s explicit release, which strands the lock on panic). `clear_stale_lock()` = unconditional remove at boot (U7 calls before spawning; safe only under the single-node pin, TD3.6).

## SD3.5 — GC reclaim safety (SR3.4)

```
objects_dir absent ⇒ Ok(zero stats)           # BR3.7/WF4.4 pinned semantics — precedes the marker gate
marker gate: dir non-empty && !.oxidean-vcs-objects exists ⇒ Config error
             (fail-closed on runtime marker deletion; empty markerless dir = fresh, proceed)
orphans = list_orphaned_objects_before(now - grace)
for o in orphans:
    if delete_object_if_unlinked(o.hash):      # conditional DELETE → bool
        remove_file(shard_path(o.hash))       # file only dies behind a real row delete
    errors → log + continue                    # per-object tolerance
walk shards:
    shape-match && mtime<grace && !vcs_get_object(hash) → remove_file
    *.tmp (tmp/ or stray) && mtime<grace → remove_file
```
`false` from the conditional delete = protected (a `vcs_file_changes.blob_hash` RESTRICT FK reference or racing link) — silent-skip, not an error; `Err` = genuine DB failure, logged. All `remove_file` calls are `io::ErrorKind::NotFound`-tolerant (LFS `store.rs:97-101` precedent): a file that vanished between stat and unlink is a no-op, not an `errors += 1`. Per-object warn logs carry `%hash` + `%error` only — never `%path`, env values, or content (SR3.5).

## SD3.6 — Quota integrity (SR3.2)

`u64` config values; `i64` row values — `u64::try_from(i64)` checked casts at the two compare sites (a negative row can't wrap to pass). Cast surface, complete: (a) `storage_bytes`/`object_count` compares, (b) the `size: u64 → i64` bind into `vcs_insert_object_if_absent` — safe only because `max_object_bytes ≤ i64::MAX` (SR3.9); state it in code. Inherited idiom caveat: `vcs_repo_object_counts` does `try_get(...).unwrap_or(0)` (vcs.rs:672) — a decode failure reads as 0, under-counting toward quota; same pattern as `repo_logical_bytes` (lfs.rs:437), accepted as house idiom. Probe-then-count is non-atomic by design: the double-charge edge (racing link lands between probe and count) wrongly denies a re-reference — rare, self-heals on retry, in the register below. `repo_quota_bytes=0` still admits `size=0` objects (`0+0 > 0` false) — pinned as intended: empty objects are free. Single-transaction quota enforcement would hold a tx across the byte stream — rejected.

## SD3.7 — Fail-close + error integrity (SR3.6/BR3.9)

Entry of `put`/`get`/`gc_once`: `db.is_skipped()` ⇒ `DbNotConfigured` (eager, not error-text). Facade `Err(String)` ⇒ `Db` variant ⇒ host-internal — genuine SQL failures are never `vcs.*` denials. `From<io::Error>` branches on `ErrorKind::NotFound ⇒ NotFound` (mid-lookup delete = domain 404), all other kinds ⇒ `Io`. Stream `Err(E)` ⇒ `Stream` variant is explicit-construction-only — a String-returning helper must NOT flow through `From<String> ⇒ Db` (stream errors are client faults, `vcs.invalid_input`). `From<String>` exists solely for the facade's `Result<_, String>` surface. Mapping to wire codes happens at U7 — the enum is the contract.

## SD3.8 — File hygiene + construct ordering (SR3.10 + ops)

`ObjectStore::new` ordering — the marker refusal precedes every write:

```
resolve dir → is-existing-regular-file ⇒ Config
            → read_dir: non-empty (excluding only `.oxidean-vcs-objects` and `lost+found`)
              && no marker ⇒ Config (foreign dir — LFS blobs share the shard shape)
            → create_dir_all(dir) → write `.oxidean-vcs-objects` marker
            → create_dir_all(tmp)          # marker BEFORE tmp: a mid-sequence crash
                                           # leaves a marked dir, not a refusing one
```
- `set_permissions(0o600)` on renamed blobs, `0o700` on `tmp/` + shard dirs (unix `#[cfg]`; non-unix inherits umask — documented). LFS precedent sets none (`crates/oxidean-git/src/cli.rs:430`, `api/ssh/host_keys.rs:41` carry the `from_mode` idiom).
- `tmp/` pinned under objects_dir (same-fs rename invariant).
- No fsync/dir-fsync (deliberate v1 hole, shared with LFS).
- Pre-marker upgrade path: a dir populated by an older build refuses; operator recovery = `touch {dir}/.oxidean-vcs-objects` after confirming ownership — documented in deployment notes.

## SD3.9 — Residual-risk register (SR3.11 carried forward — authoritative here)

| # | Risk | Disposition |
|---|------|-------------|
| R1 | Concurrent PUTs overshoot repo quota (no tx across stream) | Accepted; next pass reconciles |
| R2 | Probe-to-count race double-charges a re-reference | Rare, self-heals on retry |
| R3 | Row-less sweep `get_object`→unlink TOCTOU (row lands between) | grace-windowed; bounded by grace ≫ op latency |
| R4 | No fsync/dir-fsync | v1 hole shared with LFS |
| R5 | `tmp/` must be same filesystem (bind-mount under objects_dir breaks rename) | Config-time doc note |
| R6 | `OXIDEAN_VCS_OBJECTS_DIR` cwd-relative default | Deployment pins absolute path |
| R7 | **Empty foreign dir**: marker written into an empty LFS/packages dir; later-arriving foreign blobs become row-less sweep victims | Mitigate: `from_env` refuses when resolved dir equals resolved `OXIDEAN_LFS_DIR`/`OXIDEAN_PACKAGES_DIR`; remaining overlap documented |
| R8 | **Lock-steal then drop**: slow pass declared stale → contender acquires → original's Drop removes contender's lock → transient double-GC | Bounded by heartbeat cadence ≪ grace; GC is idempotent, harm = wasted work |
| R9 | **`lost+found` false-positive** on a fresh dedicated volume | Exempted in `new()` non-empty check (SD3.8) |
| R10 | `u64::MAX` grace/interval parses but breaks `chrono::Duration` conversion | `from_env` bounds grace to `[1, i64::MAX/1000]` (chrono's millisecond repr overflows above `i64::MAX/1000`); interval feeds `std::time::Duration` spawn-side — bound `[0, i64::MAX]` |
| R11 | Unbounded `list_orphaned_objects_before` Vec — memory + lock-duration spike on a large backlog | Upstream-accepted at TD3.4; heartbeat bounds the steal window; revisit with a LIMIT/batch clause if profiling shows it |

## SD3.10 — Corrections register (supersedes stale upstream artifact lines — fingerprints bound, do not re-edit)

- functional-spec WF1.4 → `put` returns **bare** hash (TD3.3).
- functional-spec WF1.3 → `QuotaExceeded{Repo}`+Stored runs **eager cleanup** (SD3.3).
- functional-spec WF4.4 → `VcsGcStats` adds `skipped: bool` and `errors: u32` fields.
- entities.md error enum → adds `Stream(String)`, `Internal`, `Config(String)` (LC3.3).
- entities.md:31 + functional-spec WF4 step ordering → orphan-row pass is pass 1 of the sweep.
- BR3.1/TD3.1 "shared dep: oxidean-core" → **deferred**: U3 declares no `oxidean-core` dep (nothing consumed; `cargo machete` would flag it). Added when a later unit lands a real use.

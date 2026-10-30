# Security Requirements — u3-vcs-object-store

## SR3.1 — Path-traversal guard on every fs join

Every hash reaching the filesystem validates bare 64-hex lowercase before any path join (BR3.8 — `validate_oid` clone precedent). The row-less sweep only touches names matching `{2hex}/{2hex}/{64hex}`; everything else (dotfiles, `.gc-lock`, `tmp/` internals, operator drop-ins, `lost+found`) is never a delete candidate. All walked paths canonicalize and must resolve under the objects-dir root; symlinks are never followed (BR3.6 — `wipe_lfs_dir_contents` precedent). `*.tmp` strays under shard dirs are swept too (tmp lives only under `tmp/` — anything else matching `*.tmp` past grace is a crashed-PUT remnant).

## SR3.2 — Quota fail-closed, typed denials only

Per-object cap enforced mid-stream (abort + tmp removal — never a truncated artifact); repo quota gated by the `vcs_get_object_link` membership probe so re-references don't double-charge (BR3.4). Both denials are typed `vcs.quota_exceeded`. `i64`→`u64` comparisons use checked casts — a negative/absurd row value can never wrap into a pass. On `QuotaExceeded{Repo}` where this PUT created the global entry (`Stored`): `vcs_delete_object_if_unlinked` + unlink the file eagerly — the logical quota must not be turned into a physical-disk amplifier for 24h.

## SR3.3 — Write atomicity on the fs side

Blob bytes reach their shard path only via `rename` after hash+size verification (BR3.2); a reader can never observe a partial file. `*.tmp` is never served. The GC lockfile is `create_new`-acquired, **released at pass end**, and its mtime is refreshed (heartbeat) during long passes — a present-and-young lock skips; only a lock older than `gc_grace_secs` is treated stale. Release + heartbeat keep the "two `gc_once` invocations can never interleave" invariant honest at any pass length.

## SR3.4 — GC cannot destroy reachable or live state

Reclaim requires zero live links AND entry older than `gc_grace_secs` (BR3.5). Row delete precedes file delete and is conditional (`DELETE … WHERE NOT EXISTS(link)` → `bool` — file deleted only when the row actually went). The row-less sweep probes each candidate via `vcs_get_object` (per-file indexed probe — O(files) per pass accepted at single-node scale; batch probe is a v2 option). Per-object failures log-and-skip — a single bad row never aborts the pass or cascades into other objects.

## SR3.5 — No secrets in the store or logs

Objects hold user content addressed by hash; GC stats/log lines carry counts and hashes only — never full paths, env values, or credentials. `ObjectStoreConfig` contains no secrets by construction.

## SR3.6 — Fail-closed surfaces, no error-text sniffing

`DbNotConfigured` comes only from eager `Database::is_skipped()` (BR3.9) — never from matching facade error strings. `io::ErrorKind::NotFound` maps to domain `NotFound` (deleted-mid-lookup is a 404, not a 500); all other io/db failures map to host-internal. Malformed env values are config errors at parse, never silent defaults (BR3.7). Stream chunk errors (client abort mid-body) map to `vcs.invalid_input` — a client-fault surface, not a host 500.

## SR3.7 — Tenant attribution stays per-repo

Quota accounting and link attribution are strictly `repo_id`-scoped; the global `vcs_objects` table never gains a repo column (shared CAS — FR1.5). Cross-repo dedup shares bytes but never shares links or quota (BR3.3).

## SR3.8 — No auth logic inside the unit

The store performs zero authorization — every entry point trusts an already-authorized caller (BR3.10). Cookie/PAT/classification gates live at the transport + VcsApi layers (U7).

## SR3.9 — Env-var range semantics (validated at parse)

- `OXIDEAN_VCS_GC_GRACE_SECS` ≥ 1 — `0` would make every unlinked object instantly reclaimable and every lockfile instantly stale; parse error, not silent default.
- `OXIDEAN_VCS_GC_INTERVAL_SECS` = `0` disables the GC job (house `0 disables` convention — `jobs/schedule.rs`); ≥ 1 sets cadence. Spawn-side semantics — the store only carries the value.
- `OXIDEAN_VCS_MAX_OBJECT_BYTES` ∈ [1, i64::MAX]; `OXIDEAN_VCS_REPO_QUOTA_BYTES` ∈ [0, i64::MAX] with `0` = deny-all-new-links stated. Values > i64::MAX are config errors (they could never bind to an `i64` column).
- `OXIDEAN_VCS_OBJECTS_DIR` naming an existing regular file → config error at construct, not an io error at first write.

## SR3.10 — File permissions + store marker

Blob files `0600`, `tmp/` + shard dirs `0700` (explicit `set_permissions` — private-repo blobs never ride a permissive umask; unix cfg-gated). `ObjectStore::new` writes marker file `.oxidean-vcs-objects`; `gc_once`/sweep refuses with a config error when the dir is non-empty and the marker is absent — a misconfigured dir can never eat foreign shard-shaped content (LFS blobs share the `{2hex}/{2hex}/{64hex}` shape).

## SR3.11 — Residual-risk register (owned here, all self-healing)

- Concurrent-PUT quota overshoot: both pass the check before either links — bounded by `max_object_bytes` × in-flight PUTs; single-node accepted.
- Probe→count double-charge: a racing `create_tx` link between probe and `count_for_repo` makes `storage_bytes` already include `size`; the `+size` then double-counts → wrongly denies a re-reference. Rare, succeeds on retry.
- Sweep check-vs-unlink TOCTOU: a row-less file re-PUT between probe and `remove_file` leaves a live row over missing bytes → 404 window until re-upload. Re-probe immediately before unlink shrinks the window; cannot fully close.
- No fsync/dir-fsync pin: crash-after-ack can strand a row over lost bytes — LFS shares this hole; deliberately not closed in v1.
- `tmp/` same-filesystem assumption (rename atomicity) — true by construction; a bind-mounted `tmp/` would break it (deployment note for U7 docs).
- Objects dir is process-CWD-relative by default (`./var/vcs-objects`) — matches compose CWD=`/` convention.

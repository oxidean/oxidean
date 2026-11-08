# Security Requirements — u4-vcs-changeset-core

## SR4.1 — Fail-closed input validation (BR4.4/Q2)

Every create input is validated before any DB call: `path` 1..=512 with POSIX `/`, no `\0`, no control bytes (<0x20, 0x7f), no leading/trailing `/`, no `.`/`..` segments, no `//`; `old_path` iff `rename` (same grammar); `blob_hash` iff non-delete; `mode` in the pinned octal set; `author` ≤255 in `Name <email>` shape free of `<>`, newlines, control bytes; `message` ≤65535 bytes; `parents` ≤64 (defeats silent MySQL TEXT truncation under `INSERT IGNORE` — a stored row must never diverge from what was hashed); `committed_at` normalized `%Y-%m-%dT%H:%M:%SZ`, DATETIME-range-pinned, fractions rejected. Manifest-level: `path` values unique within a changeset; `old_path ≠ path` per entry; `old_path` values unique across the manifest (a path may be the source of at most one rename per changeset — anything else is incoherent and would collide keys). `old_path` need NOT be disjoint from `path` (move-and-recreate `{rename a→b} + {add a}` is legal — the `\x01r` key suffix in SR4.6 keeps its index rows distinct). Violations → `InvalidInput` (a mid-tx PK error must never surface as `Internal`). The pinned mode set is `{33188, 33261, 40960, 16384}` — `0o120000` symlink = **40960**, correcting the FD's `41471` constant (this register supersedes; a wrong mode would both reject valid symlinks and hash a bad value into manifests).

## SR4.2 — Sentinel-exact decode (BR4.18)

Exactly four literals map typed: `vcs.not_found:parent`, `vcs.not_found:object`, `vcs.invalid_input:parents`, `vcs.invalid_input:file_change` — whole-string equality only, no substring/prefix matching. The facade consts are intentionally private; the literals are pinned in the error-mapping module. Every other DB error → `Internal` (opaque — never parsed for meaning). `ObjectNotFound` carries no hash field the sentinel can't populate (a post-sentinel probe via `vcs_get_object` on the request's candidate hashes MAY fill it — pinned path only).

## SR4.3 — Ref name grammar + id-shape rejection (BR4.9)

`^[A-Za-z0-9][A-Za-z0-9._/-]{0,254}$` with the pinned reject list (`..`, `//`, leading/trailing `/` or `.`, `@{`, `HEAD`, `refs/`-prefix) AND id-shaped names (`ChangesetId::parse` success or pure 40/64-hex — `is_changeset_id` is private to `oxidean-core`; `ChangesetId::parse` is the callable oracle, `dep:oxidean-core` per TD4.1) — creatable-but-unresolvable refs are a self-DoS (id-precedence shadows them in `log from`). All checks before any DB call → `InvalidInput`.

## SR4.4 — CAS atomicity, no pre-read TOCTOU (BR4.10)

Absent-delete/move → `RefNotFound` is derived from the CAS outcome itself (`Conflict{actual:None}`) — a post-CAS observation returned by the facade, never a pre-read that could race absent→present. Honest dialect note: only MySQL fills `actual` atomically (`SELECT … FOR UPDATE` in-tx); PG/SQLite observe via a separate SELECT after the failed guarded write — a racing create/delete can flip `actual` between write and read, but the returned value is still the correct post-state. `actual:None` ≡ absent-or-NULL-head (conflation is unreachable through this facade — NULL heads are never written). `RefConflict` carries `actual: String` (every `actual:None` route maps to RefNotFound/Ok — the Option is gone). Retried landed CAS → `RefConflict` by design (non-idempotent — clients re-read then re-CAS; the wire's `data.actual` always carries the loser-visible head).

## SR4.5 — Deterministic-id integrity (BR4.1)

`canonical_changeset()` is the single implementation — U5/U6/U7 call it; prose re-implementation is a defect (byte-shape drift silently forks id spaces and breaks AC2.2.x resubmit + AC7.4.2 import convergence). `repo` in the canonical input is a recorded deviation and REQUIRED — the global `id` PK + repo-scoped reads would otherwise let identical cross-repo content collide into an unreadable row. `idem_key` carries the full 256-bit digest (the id commits only 128 bits — the documented collision tail keeps a distinguishing key).

## SR4.6 — Derived-index integrity (BR4.19)

Default `utf8mb4_*_ci` collation PK-collides case-variant paths/refs (`README`/`readme` → silent upsert-merge = row loss). The claimed `0054` migration (all three dirs — `migration_parity`) applies `utf8mb4_bin` to `vcs_derived_index.index_key`, `vcs_refs.name`, `vcs_file_changes.path` and widens `index_key` → `VARCHAR(560)` (PK (36+64+560)×4 = 2640 < 3072). PG needs no change (equality bytewise; ordering stays deterministic per-column), SQLite none (BINARY default). Every index key is unique-per-logical-row by construction — LWW upserts can never silently merge two siblings (AC2.3.1). Load-bearing correction (AR-01 — supersedes the FD's 83-char `path_touch` pin for rename rows): the rename old-path row's key is `{sha256(old_path)[0:32]}\x01{committed_at}\x01{cs}\x01r` — the `\x01r` discriminator sorts it after the plain row while keeping it inside the `{hash32}\x01` prefix-scan block; without it `{add a} + {rename a→b}` (legal move-and-recreate) produces identical keys and LWW silently merges two logical events. Combined with SR4.1's `old_path`-uniqueness rule, no two rows in one changeset can share a key (modulo the 128-bit path-hash window — same accepted residual class as the id-prefix collision, named in SR4.11).

## SR4.7 — Cursor opacity + malformed discipline (BR4.15)

Cursors are opaque base64 payloads for every paged read — `{"n":int}` for log; `{"k":last_index_key}` for keyset reads (tree/path_history); diff's `{"k":last_emitted_path}` (diff entries emit in path order — the resume contract requires it; a `BTreeSet` candidate set makes ordering free); blame's `{"k":shard_key,"off":intra_shard_offset}`; ref-list's `{"k":last_ref_name}` (base64-wrapped like the rest for uniformity — a name is a valid `>` bound regardless). Malformed decode → `CursorMalformed` → `vcs.invalid_input`; a wrong-but-decodable cursor yields an empty/next page, never an oracle. path_history orders by `committed_at` embedded in the key — second-precision; same-second touches order by changeset id (this wording supersedes WF7.1's "key order IS chronological").

## SR4.8 — Bounded walks (NFR1 honest bounds)

`log` in-walk `seen` cap 65536 → `FrontierTooLarge`; `merge_base` mark set cap 250k → `FrontierTooLarge`; `diff` candidate walk visited cap 250k → `FrontierTooLarge`; `blame` touch-collection cap 65,536 touches (path touches + `old_path` rename-chain hops, injectable) → `FrontierTooLarge` — "bounded by path history" is a data-bound, not a cap, and a hot file touched by every commit is O(history) in memory + `ObjectStore::get` opens; `tree` materialize O(tree) per miss. Cap constants are named and injectable (constructor/config params) so tests exercise `FrontierTooLarge` without 250k-node fixtures. Derived-index payload bounds: blame shard ≤32KB; every kind stays under the 64KB MySQL `TEXT` ceiling — an oversize payload is a silent-truncation vector, same class as the message/parents bounds in SR4.1. `vcs_list_file_changes` is unpaginated — u7's 10k manifest cap is the bound (transitively caps per-changeset work). All other bounds are named and typed — no unbounded-by-accident path (the O(#refs) ref-resolution scan is an accepted schema-unbounded residual, named in SR4.11 — the only residual with neither cap nor amortization; the other accepted cost residuals — cold fold O(history), skip-replay O(pages²·limit), blame O(#touches)-inside-cap — are named there too).

## SR4.9 — No auth inside; fail-closed config (BR4.16)

Zero authorization in the unit — callers are already-authorized (u7 owns session/PAT/classification). `DbNotConfigured` only from eager `Database::is_skipped()` — never error-text sniffing (BR3.9 precedent). `Internal` never stringifies internals into wire data — u7 maps variants to codes.

## SR4.10 — Payload self-description is the semantic guarantee

Hashed-path keys are the index mechanism, not the proof — every hashed-path read filters `payload.path == arg` (path_touch rows, blame shards; the 128-bit hash window is trusted but never alone). Blame shards carry `path` in payload for the same guard. `tree_entry` payloads carry `{mode,blob}` only — no path — so tree reads guard on the raw-path suffix of `index_key` (`{cs}\x01{path}`) instead; the hashed-path rule does not apply there. Index payloads duplicate path/cs/committed_at so reads never re-join for their own fields.

## SR4.11 — Residual-risk register (owned here)

- **merge_base may pick a dominated ancestor** on multi-branch topologies (minimal-BFS-distance, deterministic, contract-adequate single id) — documented; u5 merge consumes it knowingly (u5 may pin a domination check if merge quality demands).
- **Skip-replay log cursor is O(n+limit) point reads/page** — total O(pages²·limit) on full-history page-outs; accepted (PK probes are cheap; deep paging is rare).
- **`lo` seed membership**: merge_base marks `lo` AND its ancestors (BFS seed = {lo}) — the dominant linear/fast-forward case returns `lo` itself, correct per AC4.2.2.
- **tz-offset loss on import** — committed_at normalizes to UTC; AC7.4.1's byte-identical round-trip is foreclosed at this schema (u6 handoff note).
- **Ref-existence scan is O(#refs) per resolution** — ref count is unbounded in schema; `log from` resolution pages the entire ref list per call (⌈N/500⌉ pages). Accepted: no early-stop exists (PG locale collation isn't bytewise); u7 rate-limits.
- **Blame touch-collection is capped at 65,536 touches** (SR4.8) but still O(#touches) memory + `ObjectStore::get` opens per cache-miss call — a hot file touched by every commit is O(history) inside the cap; accepted with the cap as the tripwire.
- **Log page-out depth ceiling** — the `seen` cap 65536 forecloses `log` page-out deeper than ~32-64k visited nodes: `next_cursor` chains trip `FrontierTooLarge` on histories past the ceiling (the register's quadratic-cost entry covers cost; this is the hard depth bound). Accepted; u7 documents the ceiling in the wire contract.
- **128-bit id prefix collision** reselects the other content's row as Ok — cryptographically negligible; `idem_key`'s full digest distinguishes. The same accepted class covers **path-hash collisions** (`sha256(path)[0:32]` in `path_touch`/blame keys) — distinct paths colliding in the window merge `index_key`s silently; SR4.10's `payload.path==arg` filter drops the loser's history rather than corrupting it.
- **Blame/diff read ObjectStore bytes** — read-path only (C6-granted u3→u4 edge); create/write path never sees bytes.
- **Cold `tree()`/`file_at()` fold is O(history length)** on a repo with no `tree_snap` — first-parent walk to root per miss until the first snapshot lands.
- **GC-safety invariant on index payloads**: derived-index payloads may reference only manifest-covered blob hashes (BR4.6 object-links + RESTRICT + immutable changesets keep them reachable). A future index kind storing a non-manifest hash would silently break GC safety — flagged for any kind addition.
- **utf8mb4_bin behavior unverified in CI** — the `db-dialects` lane executes the 0054 ALTERs on real MySQL 8.4 (migrate-only coverage); case-variant coexistence and `>` keyset semantics under `utf8mb4_bin` get no execution until the pinned `dialect_vcs` test lands (TD4.8).

## SR4.12 — Observability on typed-error and index-write paths

Structured `tracing` on typed-error returns (create_tx rollback causes, CAS conflicts, `FrontierTooLarge`/`CursorMalformed` trips) and index-write counts — matching the `object_store/gc.rs` warn!/info! precedent (`tracing` is already a server dep). Logs carry ids/hashes/counts only — never path, message, or author payload. u7 owns request logging but cannot see inside the stores; this is the unit's own instrumentation seam.

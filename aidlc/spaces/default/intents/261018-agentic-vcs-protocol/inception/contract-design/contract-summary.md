# Contract Summary — Agentic VCS Protocol

Twelve contracts: three external (RPC family, object routes, MCP descriptors), one external-adjacent feed contract (activity rows), and the internal seams (persistence façade, per-provider engine interfaces, interop ports, client library, generated client provenance). Wire types live in `oxidean-core`; every `vcs.*` input carries `{owner, name}` as separate fields; the envelope is the untagged `RpcResponse` (`{ok:true,data}` / `{ok:false,error:{code,message,data?}}`).

**Auth scope note (resolves an upstream ambiguity):** session cookies DO authorize `vcs.*` RPC procedures — `vcs.status`/`vcs.repo_enable` are called by the settings page through `@oxidean/api-client` with `credentials:"include"`. The cookie denial (FR14.1/AC8.5.1) is scoped to the `/api/vcs/*` object routes only — headless byte transport never rides a browser session.

## Contracts table

| # | Provider Unit | Consumer | Mechanism | Owner |
|---|---------------|----------|-----------|-------|
| C1 | u7-vcs-api-surface | External: `ox` CLI, MCP host, web, third-party agents | JSON-RPC over `POST /api/rpc` (+ `/api/rpc/ws` WS variant; advertised via `system.manifest`) | u7 |
| C2 | u7-vcs-api-surface (object routes) | External: `ox`, agents | REST `GET`/`POST /api/vcs/{owner}/{name}/objects[/{hash}]` | u7 |
| C3 | u9-vcs-mcp | External: MCP clients | tool descriptors on `/api/mcp` | u9 |
| C4 | u7-vcs-api-surface (ActivityRecorder) | u10-vcs-web-surfaces, u2-vcs-persistence | shared `repo_activity` row schema + `vcs.*` kind constants + nullable push columns on `insert_repo_activity` | u7 |
| C5 | u2-vcs-persistence | u3, u4, u6, u7 | shared-schema: façade method signatures + row DDL | u2 |
| C6a | u3-vcs-object-store | u4-vcs-changeset-core | `ObjectLink` insert inside the create tx; blob-hash verify | u3 |
| C6b | u4-vcs-changeset-core | u5, u6, u7 | changeset create path + lineage/index reads | u4 |
| C6c | u5-vcs-merge-engine | u7 | merge/rebase signatures → `vcs.conflict` envelope data | u5 |
| C6d | u6-vcs-git-interop | u7 | import/lazy-import/export entry points | u6 |
| C7 | u6-vcs-git-interop (port) | oxidean-api (implements) | trait `ProtectionChecker` + `Arc<dyn GitBackend>` | u6 |
| C8 | u8-vcs-cli (`WorkingCopy`) | `oxidean-cli` | client-feature Rust API + injected transport | u8 |
| C9 | `oxidean-core` wire types | packages/api-client | generated via `make rpc-gen` — provenance contract | u1 |

## C1 — `vcs.*` RPC family (external)

17 procedures, callable over `POST /api/rpc` and the `/api/rpc/ws` WS variant (AC8.1.1), advertised in `system.manifest.procedures` (FR10.3). Each is explicitly classified in `proc_classes` — read tier (RepoRead): `vcs.status`, `vcs.changeset_get`, `vcs.log`, `vcs.diff`, `vcs.tree`, `vcs.file_get`, `vcs.blame`, `vcs.path_history`, `vcs.merge_base`, `vcs.ref_list`; write tier (RepoWrite): `vcs.changeset_create`, `vcs.merge`, `vcs.rebase`, `vcs.ref_update`, `vcs.git_import`, `vcs.git_export`; admin: `vcs.repo_enable`. Unlisted → `SessionOnly` fail-closed (AC8.6.1).

**Error taxonomy (AC8.1.4, wire-stable):** `vcs.disabled`, `vcs.not_found`, `vcs.invalid_input`, `vcs.ref_conflict` (`data:{actual}` carries the stored ref value), `vcs.conflict` (`data:{conflicts:[{path,kind,detail?}]}` — the typed merge/rebase denial, never an `ok:true` result), `vcs.no_common_ancestor` (merge_base/merge/rebase when no ancestor exists), `vcs.quota_exceeded` (object size + repo-quota denials — the AC3.1.3/AC3.5.3 mapping), `vcs.too_large` (RPC-side caps: changeset_create >10_000 changes, inline content >256 KiB/entry), `vcs.import_state` (git_import on malformed cursor or a concurrent in-progress import), `vcs.unauthenticated`, `vcs.forbidden` — plus shared `repo.archived`, `db.not_configured`, `rpc.bad_input`.

**Idempotency:** `vcs.changeset_create` is idempotent (byte-identical resubmit → same id, AC2.2.x); object POST dedups. `vcs.ref_update` is intentionally NON-idempotent (AC5.1.3): a retried landed call fails `vcs.ref_conflict`. Conflicts/quota/validation/denials are terminal — never retried.

```yaml
# RPC procedure signatures — request: {"procedure": "<name>", "input": {...}}
procedures:
  vcs.repo_enable:
    class: admin
    in:  {owner: string, name: string, enabled: boolean}
    out: {enabled: boolean}
  vcs.status:
    class: RepoRead
    in:  {owner: string, name: string}
    out: "{enabled: boolean, heads: [{name, changeset_id}], changeset_count: int, object_count: int, storage_bytes: int}"
    note: never-enabled repo → vcs.disabled error envelope (absent flag row denies all; enabled:false denies writes only)
  vcs.changeset_create:
    class: RepoWrite
    in:  "{owner, name, parents: [changeset_id], message: string, author?: {name, email}, changes: [{path, kind: add|modify|delete|rename, mode: \"100644\"|\"100755\"|\"120000\", old_path?, blob?: sha256, content_b64?: string}]}"
    note: "author optional — resolved server-side from the authenticated platform user when absent (FR2.2); verbatim author is meaningful only on the internal import path. Exactly one of blob (existing object) or content_b64 (inline ≤256 KiB) per non-delete change; delete carries neither — two transports are distinct fields (AC2.1.2)"
    out: {changeset_id: string, manifest_hash: string}
  vcs.changeset_get:
    class: RepoRead
    in:  {owner: string, name: string, id: "changeset_id|git_oid"}
    out: "{changeset_id, parents: [], author, message, committed_at, manifest: [{path, kind, mode, old_path?, blob}]}"
    note: git-oid miss triggers lazy import then re-resolves (AC7.1.2)
  vcs.log:
    class: RepoRead
    in:  {owner: string, name: string, from: "ref_name|changeset_id|git_oid", limit: "int?", cursor: "string?"}
    out: "{entries: [changeset rows as changeset_get], cursor: string?}"
    note: "git-oid from-miss triggers lazy import (AC7.1.2); cursor absent = end"
  vcs.tree:
    class: RepoRead
    in:  {owner: string, name: string, changeset_id: changeset_id, path_prefix: "string?", limit: "int?", cursor: "string?"}
    out: "{entries: [{path, mode, blob}], cursor: string?}"
  vcs.diff:
    class: RepoRead
    in:  {owner: string, name: string, from: changeset_id, to: changeset_id, paths: "[string]?", limit: "int?", cursor: "string?"}
    out: "{entries: [{path, kind, old_mode?, new_mode?, old_blob?, new_blob?, patch?|{truncated:true}}], cursor: string?, totals: {files, additions, deletions}}"
    note: "bounded — limit ≤500 files (paginated by file), patch ≤256 KiB/file else truncated marker; paths is a multi-value filter (CLI --paths repeatable)"
  vcs.blame:
    class: RepoRead
    in:  {owner: string, name: string, path: string, at: "changeset_id?", limit: "int?", cursor: "string?"}
    out: "{entries: [{changeset_id, author, content}], cursor: string?}"
    note: "at omitted → `main` ref tip when present, else the sole head when exactly one ref exists; no refs → vcs.not_found; ≥2 refs without main → vcs.invalid_input (pass at explicitly)"
  vcs.path_history:
    class: RepoRead
    in:  {owner: string, name: string, path: string, limit: "int?", cursor: "string?"}
    out: "{entries: [{changeset_id, kind, path, committed_at}], cursor: string?}"
  vcs.merge_base:
    class: RepoRead
    in:  {owner: string, name: string, a: changeset_id, b: changeset_id}
    out: {merge_base: "changeset_id"}
    errors: "vcs.no_common_ancestor when no ancestor exists (AC4.2.2)"
  vcs.file_get:
    class: RepoRead
    in:  {owner: string, name: string, changeset_id: changeset_id, path: string}
    out: "{blob, mode, size, content_b64: string|null, too_large: bool}"
    note: "content_b64 inline only when size ≤262144 B; else too_large:true → fetch via C2 GET"
  vcs.merge:
    class: RepoWrite
    in:  {owner: string, name: string, a: changeset_id, b: changeset_id, message: "string?"}
    out: "{changeset_id: string, fast_forward?: bool}   # ancestor/descendant pair → the descendant (a or b; no merge node)"
    errors: "vcs.conflict{conflicts:[{path,kind,detail?}]} | vcs.no_common_ancestor"
    note: "field names a/b supersede the components.md internal signature merge(ours,theirs) — wire aligns with vcs.merge_base and the pinned MCP example {a,b} (mockups.md:200); internal signature unchanged"
  vcs.rebase:
    class: RepoWrite
    in:  "{owner, name, changeset_id: changeset_id, new_parent_id: changeset_id, message: \"string?\"}"
    out: {head: changeset_id}
    errors: "vcs.conflict{conflicts:[…]} | vcs.no_common_ancestor | vcs.invalid_input on descendant/cycle target (AC2.4.3)"
    note: "pinned AC2.5.1 shape — replays ONE changeset's changed files onto a new parent (ox vcs rebase <cs> --onto <cs>); chain replay is a client loop, not a wire range param"
  vcs.ref_list:
    class: RepoRead
    in:  {owner: string, name: string, limit: "int?", cursor: "string?"}
    out: "{entries: [{name, changeset_id, updated_at}], cursor: string?}"
  vcs.ref_update:
    class: RepoWrite
    in:  "{owner, name, ref: string, expected_old: changeset_id|null, new: changeset_id|null}"
    out: "{ref: string, changeset_id: string|null}"
    note: "expected_old:null = create-only; new:null = delete; non-idempotent (AC5.1.3) — a retried landed call fails vcs.ref_conflict{data:{actual}}"
  vcs.git_import:
    class: RepoWrite
    in:  {owner: string, name: string, from: "ref|git_oid?", cursor: "string?", limit: "int?"}
    out: "{imported: int, cursor: string?, done: bool, heads: [{name, git_oid, changeset_id}]}"
    limits: "≤1000 commits/call; resumable via cursor; vcs.import_state on malformed cursor / concurrent in-progress import"
  vcs.git_export:
    class: RepoWrite
    in:  {owner: string, name: string, head: changeset_id, branch: string, force: "bool?"}
    out: "{branch: string, git_oid: string}"
    errors: "vcs.forbidden on protection denial (data:{verdicts:[…]}) — identical verdict path to git push (AC7.2.2); force overrides fast-forward only"
limits:
  pagination: "limit ≤500 entries/call; opaque cursor field; absent cursor = end"
  rpc_body:   "POST /api/rpc body cap bounds inline content_b64 — per-entry inline ≤256 KiB, whole-request cap raised above that with headroom (same pattern as C2's POST body limit)"
```

## C2 — Object routes (external)

PAT (Basic/Bearer) and OAuth access tokens only — session cookies are always denied (FR14.1, AC8.5.1). Success GET is raw `application/octet-stream`; every failure is the JSON envelope with real statuses. Upload is `POST` to the collection (server computes the hash — the pinned FR11.2 shape), dedup no-op on re-upload.

```yaml
object-routes:
  "GET /api/vcs/{owner}/{name}/objects/{hash}":
    success: "200 application/octet-stream; Cache-Control: immutable, max-age=31536000"
    errors:
      "401": vcs.unauthenticated (cookie-only or missing credentials)
      "403": "vcs.disabled (never-enabled repo only — enabled:false still serves reads, AC1.2.1) | vcs.forbidden (insufficient class)"
      "404": vcs.not_found
  "POST /api/vcs/{owner}/{name}/objects":
    body: "raw blob bytes (server computes sha256 → returns hash)"
    success: "200 {\"ok\":true,\"data\":{\"hash\":\"sha256:…\"}} — dedup no-op on re-upload"
    errors:
      "401": vcs.unauthenticated
      "403": "vcs.disabled | vcs.forbidden"
      "413": "vcs.quota_exceeded (per-object size cap AND repo-quota — AC3.1.3/AC3.5.3 pinned mapping)"
    body-limit: "DefaultBodyLimit raised ABOVE OXIDEAN_VCS_MAX_OBJECT_BYTES (e.g. 512 MiB limit vs 256 MiB cap) so the typed quota check runs before Axum's untyped 413 — LFS precedent shape (app.rs:389)"
proc-classes-alias: "route authorization classifies under proc_classes route-alias names (bearer.rs:735 convention); these aliases never appear in PROCEDURES"
```

## C3 — MCP tool descriptors (external)

13 tools (the FR13.1 dozen + `vcs.changeset_get` — contract-design resolves the parked question: agents need point reads for lazy-import debugging). Separate `owner`/`name` args; Access classes mirror C1; domain denials map to `isError` tool content.

```yaml
tools: [vcs_status, vcs_log, vcs_diff, vcs_tree, vcs_file_get, vcs_blame,
        vcs_path_history, vcs_merge_base, vcs_changeset_create, vcs_merge,
        vcs_rebase, vcs_ref_list, vcs_changeset_get]
absent-by-design: [vcs_repo_enable, vcs_ref_update, vcs_git_import, vcs_git_export, checkout]
```

## C4 — `repo_activity` row contract (u7 → u10/u2)

One row per mutating op via the façade. **Decision (was parked):** `insert_repo_activity`'s push columns become nullable — `ref_name`, `before_oid`, `after_oid`, `commits_count` are `Option<>`; ref-free ops write NULLs. `push_type` carries the `vcs.*` kind constants from `oxidean-core`; the host's `VALID_PUSH_TYPES` allow-list extends from them.

```yaml
op-to-row:
  vcs.repo_enable:      {push_type: vcs.repo_enable, ref_name: null, before_oid: null, after_oid: null, commits_count: null}
  vcs.changeset_create: {push_type: vcs.changeset_create, ref_name: null, before_oid: null, after_oid: "<changeset_id>", commits_count: 1}
  vcs.merge:            {push_type: vcs.merge, after_oid: "<result changeset_id>", commits_count: "<1|0 fast-path>"}
  vcs.rebase:           {push_type: vcs.rebase, after_oid: "<head changeset_id>", commits_count: 1}
  vcs.ref_update:       {push_type: vcs.ref_update, ref_name: "<ref>", before_oid: "<expected_old|prior>", after_oid: "<new|null>", commits_count: null}
  vcs.git_import:       {push_type: vcs.git_import, after_oid: "<last imported changeset_id>", commits_count: "<imported>"}
  vcs.git_export:       {push_type: vcs.git_export, ref_name: "<branch>", before_oid: "<prior branch git_oid|null>", after_oid: "<git_oid>", commits_count: "<exported count>"}
rendering: "u10 renders vcs.* kinds via actionCopy/commitHref updates; cs_ ids never route to /commit/{sha}"
```

## C5 — `VcsPersistence` façade (u2 → u3/u4/u6/u7)

All methods `Database`-by-value, `skipped()` → `db.not_configured`. Signature-level schema; dialect DDL lives in migrations ×3.

```yaml
facade:
  repo_surface_flag: "{get, upsert}"
  object_entry:      "{get_by_hash, insert_if_absent, list_orphaned_before(grace_ts)}"
  object_link:       "{insert (same tx as changeset), count_for_repo, live_hashes()}"
  changeset:         "{create_tx(changeset, file_changes, object_links, index_upserts), get, parents_of, find_by_idempotency}"
  ref:               "{list, cas_update(expected_old, new), delete}"
  derived_index:     "{upsert_rows (merge semantics — concurrent same-parent safe), query(kind, key, cursor)}"
  import_link:       "{insert, lookup(git_oid), cursor_page}"
  repo_activity:     "{insert_repo_activity — push columns NULLable per C4}"
```

## C6 — Engine interfaces (provider-owned)

Rust-level contract inside `oxidean-vcs` (server feature); each provider unit owns its own interface signature:

```yaml
object_store (u3 → u4):    "put(reader) -> Result<{hash, Stored|Dedup}>; get(hash) -> stream; verify(hash); insert_links_tx(repo_id, [hash])"
changeset_store (u4 → u5/u6/u7): "create(input) -> Result<ChangesetId>; get(id); lineage(id); resolve_git_oid(oid) -> Option<ChangesetId>"
ref_registry (u4 → u7):    "cas(name, expected_old, new) -> Result<Ref|RefConflict{actual}>"
read_index (u4 → u5/u7):   "update_for(changeset) /* called inside create tx */; log/tree/diff/blame/path_history/merge_base queries"
merge_engine (u5 → u7):    "merge(ours, theirs) -> Clean(changeset_input)|Conflicts[Conflict]|NoAncestor; rebase(changeset_id, new_parent_id)"
git_interop (u6 → u7):     "import_chain(from, cursor, limit) -> ImportPage; export(head, branch, force); lazy_import(git_oid) -> ChangesetId"
```

## C7 — Interop ports (u6 ↔ oxidean-api)

```yaml
protection_checker: "trait defined in oxidean-vcs, impl injected by host: check(repo, refname, old_oid, new_oid, actor) -> ProtectionVerdict — identical verdict path to git push"
host_supplies: "Arc<dyn GitBackend> — hook-driven fast_forward_ref / force_update_ref only; never bare update_ref"
```

## C8 — Client library (u8)

```yaml
working_copy: "checkout(transport, repo, changeset_id, filter) -> Result<CheckoutResult {files_written, blobs_fetched, cache_hits, skipped_outside_filter}>"
cli_surface:  "16 commands → Out::Rpc; errors ox: <code>: <message> on stderr; vcs.unauthenticated|vcs.forbidden → exit 3 via error_exit_code match-list extension"
transport:    "injected by oxidean-cli (POST /api/rpc, Bearer PAT); client feature compiles with zero DB/git deps"
```

## C9 — Generated client provenance

`packages/api-client` is regenerated by `make rpc-gen` whenever C1 wire types change; committed with its producer diff. Never hand-edited (per repo boundary rules).

## `OXIDEAN_VCS_*` operator surface (all defaults frozen)

| Var | Default | Notes |
|-----|---------|-------|
| `OXIDEAN_VCS_OBJECTS_DIR` | `./var/vcs-objects` | sha-sharded CAS dir; never shares a dir with other content stores; joins factory-reset sweep |
| `OXIDEAN_VCS_GC_INTERVAL_SECS` | `3600` | GC job cadence |
| `OXIDEAN_VCS_GC_GRACE_SECS` | `86400` | POST→commit protection window |
| `OXIDEAN_VCS_MAX_OBJECT_BYTES` | `268435456` (256 MiB) | per-object cap → `vcs.quota_exceeded`; route body limit sits above it |
| `OXIDEAN_VCS_REPO_QUOTA_BYTES` | `10737418240` (10 GiB) | per-repo logical-bytes quota → `vcs.quota_exceeded` |

## Contract ownership rules

- Provider unit owns its spec block; changes require that unit's Bolt.
- Breaking wire change ⇒ SemVer `!` commit + `wire_version` minor bump (0.x discipline); additive fields are always safe (consumers ignore unknown fields — no `deny_unknown_fields` anywhere on the wire).
- Internal seams (C5–C8) change only with their owning unit; cross-unit callers see signatures, not internals.

## Open questions

| Contract | Question | Blocks |
|----------|----------|--------|
| — | None — all ten parked items frozen (status schema, `expected_old`/`new` ref args per the pinned wire example, conflict payload `{path,kind,detail?}` in a `vcs.conflict` envelope, diff bounds, file_get cap + `mode`, import bound/cursor, activity nullability, 13-tool MCP set, `OXIDEAN_VCS_*` defaults, RPC/route caps) | — |

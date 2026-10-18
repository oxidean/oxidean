# Refined Mockups — Side-by-Side Agentic VCS Protocol

Surface inventory per the ratified interview (Q1=B): machine-facing surfaces (CLI, MCP, RPC envelopes) plus exactly one Octane control — the repo-settings VCS toggle. No web history views (O5). Consumes `stories.md` (34 stories, G1–G8) and `requirements.md` (FR1–FR16).

Wire conventions the mockups are pinned to (verified against the codebase):
- Repo targeting uses separate `{owner, name}` fields everywhere (`repo_types.rs` request shapes, MCP `owner`/`name` tool props, PAT repo-target resolution) — a combined `"repo":"owner/name"` string does not exist on the wire and would fail-open the MCP FG-grant check.
- CLI output rides `Out::Rpc {envelope, pretty}` — the RPC variant that prints the envelope verbatim under `--json` and drives `rpc::error_exit_code`. (`Out::Report` is the local-only variant that always exits 0 — never used for `vcs.*` calls.) Error text on stderr follows the existing `ox: <code>: <message>` format; under `--json`, error envelopes also go to stderr.
- Exit codes come from the existing `error_exit_code` mapping (auth-class → 3, everything else → 1). That mapping matches a literal code list today — `vcs.unauthenticated`/`vcs.forbidden` must be added to it (or a class rule added) for the auth-tier 3 shown here; otherwise they exit 1 like other domain errors. No per-code numbers are invented beyond that extension.

## S1 — Repo settings: VCS surface toggle (US1.1, US1.2)

One control appended to the existing `Repository settings` page (`[owner]/[repo]/settings`), in its own section. Octane `.tsrx`, existing settings-section styling.

```
┌─ Source control protocols ─────────────────────────────────────────────┐
│                                                                        │
│  Git is always available on this repository.                           │
│                                                                        │
│  [x] Enable the VCS protocol                                           │
│      Agents and automation can commit through the `vcs.*` API without  │
│      clones or checkouts. History is stored as server-side changesets. │
│      Turning this off stops new writes; existing history stays         │
│      readable. Learn more → docs                                       │
│                                                                        │
│                                          [ Save changes ]              │
└────────────────────────────────────────────────────────────────────────┘

States:
  loading                — checkbox indeterminate/inert while vcs.status resolves
                           (TanStack Query isLoading). On a never-enabled repo,
                           vcs.status returns vcs.disabled → the control renders
                           as unchecked-default (not the error state).
  disabled (unchecked)   — default; copy unchanged
  enabled (checked)      — after successful vcs.repo_enable {enabled:true}
  saving                 — button disabled, "Saving…" label
  success                — control shows new value + transient "Saved" confirmation
  error                  — inline red text under the control:
                           "Couldn't update: vcs.forbidden" / "db.not_configured"
  read-only              — checkbox disabled, muted: repo archived OR caller
                           lacks admin class (tooltip names which)
```

Persona: Sana. State transitions: save → `vcs.repo_enable`; denial → inline typed error text; archived repo → control disabled.

## S2 — `ox vcs` CLI: operator & reviewer flows

Human output shown; `--json` prints the RPC envelope verbatim and `{ok:false}` drives the exit code (`Out::Rpc`).

### S2.1 Enable / status (Sana, US1.1, US8.4)

```
$ ox vcs enable myorg backend
VCS protocol enabled on myorg/backend (empty history, ready for commits)

$ ox vcs enable myorg backend --disable
VCS writes disabled on myorg/backend (history remains readable)

$ ox vcs status myorg backend
Repository: myorg/backend
VCS:        enabled
Changesets: 142
Refs:       main → cs_01H…  agent/task-77 → cs_01K…

$ ox vcs status myorg backend --json     (pretty-printed — Out::Rpc emits to_string_pretty)
{"ok":true,"data":{"enabled":true,"changesets":142,"refs":{"main":"cs_01H…","agent/task-77":"cs_01K…"}}}
```

(`vcs.status`'s exact field set is provisional — requirements pin only "live + empty history" (AC1.1.2); contract-design owns the shape. The `git_oid ↔ changeset_id` map from AC7.1.1 is queried via a dedicated call — e.g. `ox vcs import --lookup <git-oid>` — not a status field.)

### S2.2 Agent commit (Ava, US2.1, US2.2, US2.6)

`--change` grammar: `path:kind:mode:blobspec` — `kind` ∈ add|modify|delete|rename; `mode` = file mode (`100644` default, `100755` executable, `120000` symlink); `blobspec` = `@sha256:…` (prior object PUT) or `@file:<path>` (inline contents — the second transport field of AC2.1.2); rename entries carry `oldpath>newpath`.

```
# root commit — no parent (AC2.1.4 bootstrap)
$ ox vcs commit myorg backend -m "initial" \
    --change README.md:add:100644:@file:./README.md
cs_01A…

$ ox vcs commit myorg backend --parent cs_01K… -m "add rate limiter" \
    --change src/limiter.ts:modify:100644:@sha256:7f3a… \
    --change 'src/index.ts>src/app.ts:rename:100644:@sha256:b21c…' \
    --change scripts/deploy.sh:add:100755:@file:./deploy.sh
cs_01M4X8N2…        (idempotent: identical re-submit prints the same id)

$ ox vcs commit myorg backend --parent cs_01K… -m "concurrent agent work"
cs_01N7…            (same parent as above — distinct id; forks are normal, AC2.3.1)
```

### S2.3 History, tree, diff, blame, point reads (US4.x)

```
$ ox vcs log myorg backend --from main --limit 20
cs_01M4X8N2…  2026-10-18T05:12Z  ava-agent   add rate limiter
cs_01K9…      2026-10-18T04:58Z  ava-agent   refactor middleware        (2 parents)
…
Next cursor: cur_eyJ…   (repeat with --cursor cur_eyJ…, or --all to page to the end)

$ ox vcs tree myorg backend cs_01M4X8N2… --limit 200
src/limiter.ts   100644  sha256:7f3a…
src/app.ts       100644  sha256:b21c…
…                               (paginated per AC4.1.2)

$ ox vcs diff myorg backend cs_01K9… cs_01M4X8N2…
 src/limiter.ts  (modified)   +41 −3
 src/app.ts      (renamed from src/index.ts)

$ ox vcs blame myorg backend src/limiter.ts cs_01M4X8N2…
cs_01K9…  ava-agent   1   use std::…
cs_01M4X8N2… ava-agent  2   pub struct RateLimiter {

$ ox vcs history myorg backend src/limiter.ts        # vcs.path_history
cs_01M4X8N2…  2026-10-18T05:12Z  modified
cs_01A…          2026-10-17T22:01Z  added

$ ox vcs merge-base myorg backend cs_01A… cs_01B…
cs_0091…

$ ox vcs cat myorg backend src/app.ts --at cs_01M4X8N2… > app.ts   # vcs.file_get bytes
$ ox vcs show myorg backend cs_01M4X8N2…                            # vcs.changeset_get manifest
```

### S2.4 Ref update CAS + conflicts (US5.1, US5.2)

```
$ ox vcs ref list myorg backend
main          → cs_01H…
agent/task-77 → cs_01K…

$ ox vcs ref update myorg backend agent/task-77 --expect cs_01K… --to cs_01M4…
agent/task-77 → cs_01M4…

$ ox vcs ref update myorg backend main --expect cs_01H… --to cs_01M4…
ox: vcs.ref_conflict: ref "main" is at cs_01J7…, not cs_01H…        (exit 1)

$ ox vcs ref update myorg backend agent/task-78 --expect-absent --to cs_01N7…   # create
$ ox vcs ref update myorg backend agent/task-77 --delete                         # delete (null target)
```

### S2.5 Merge / rebase conflicts (US2.4, US2.5)

```
$ ox vcs merge myorg backend cs_01A… cs_01B…
ox: vcs.conflict: 2 conflicting paths
  src/config.ts    (text — diff3 conflict, markers in --json conflicts list)
  assets/logo.png  (binary — file-level conflict)
                                                        (exit 1)

$ ox vcs rebase myorg backend cs_01K9… --onto cs_01M4X8N2…
cs_01P3…          (replayed changeset id; conflicts surface the same {path,kind} list — AC2.5.2)
```

### S2.6 Checkout (Ravi/Kit, US6.x)

```
$ ox vcs checkout myorg backend cs_01M4X8N2… --paths 'src/**' --dir ./wc
materialized 37 files (skipped 1,204 blobs outside filter)

$ ox vcs checkout myorg backend cs_01M4X8N2… --paths 'src/**' --dir ./wc2
materialized 37 files (0 fetched — all cached)                       (US6.2 cache hit)
# executable bits and symlinks applied (US6.3); modes preserved on Linux/macOS
```

### S2.7 Git interop (US7.1, US7.2)

```
$ ox vcs import myorg backend          # explicit, deterministic
imported 1,204 commits → changesets
$ ox vcs import myorg backend --lookup a1b2c3…        # git_oid → changeset id
cs_01K9…

$ ox vcs export myorg backend cs_01M4… --to main     # the trust-loop landing
exported 3 commits; main → a1b2c3… (one branch move — AC7.2.1)

$ ox vcs export myorg backend cs_01M4… --to main     # on a protected branch:
ox: vcs.forbidden: branch main is protected (requires PR review)
# identical verdict text to a rejected `git push` on the same branch (AC7.2.2)
```

### CLI state/exit-code map

| Surface state | Text behavior | `--json` | Exit |
|---|---|---|---|
| success | one-line or table summary on stdout | `{"ok":true,"data":…}` on stdout | 0 |
| empty result | explicit zero-state line — `log` prints `no changesets`, `ref list` prints `[]` equivalent, `status` shows `Changesets: 0` | `{"ok":true,"data":{"entries":[]}}`-shape | 0 |
| typed-domain error (`vcs.conflict`, `vcs.ref_conflict`, `vcs.not_found`, `vcs.disabled`, `vcs.invalid_input`, `vcs.no_common_ancestor`, `vcs.quota_exceeded`, `db.not_configured`) | `ox: <code>: <message>` on stderr | `{"ok":false,"error":{…}}` on stderr | per `error_exit_code` (1 for domain errors) |
| auth (`vcs.unauthenticated`, `vcs.forbidden`) | `ox: <code>: <message>` on stderr | envelope on stderr | 3 (auth tier, beside `auth.*`) |
| local/transport failure (checkout target conflict — AC6.1.3; cache corruption; network) | `ox: <local-code>: <message>` on stderr (client-side errors, not `vcs.*` codes) | `{"ok":false,"error":{…}}` synthesized client-side | 1 |

## S3 — MCP tool call shape (Ava, US8.3)

12 tools (`vcs_status`, `vcs_log`, `vcs_diff`, `vcs_tree`, `vcs_file_get`, `vcs_blame`, `vcs_path_history`, `vcs_merge_base`, `vcs_changeset_create`, `vcs_merge`, `vcs_rebase`, `vcs_ref_list`) — descriptors emitted by `oxidean-vcs`, wrapped with `Access` gating in `oxidean-api::mcp` (AC8.3.3). Args carry separate `owner`/`name` fields (the wire convention the MCP grant check reads).

```
tools/call  vcs_changeset_create
  args:   {"owner":"myorg","name":"backend","parents":["cs_01K…"],"message":"…",
           "changes":[{"path":"src/x.ts","kind":"modify","mode":"100644","blob":"sha256:…"}]}
  result: {"changeset_id":"cs_01M4X8N2…"}                      (success content)

tools/call  vcs_merge {"owner":"myorg","name":"backend","a":"cs_…","b":"cs_…"}
  result: isError content — {"code":"vcs.conflict","conflicts":[{"path":"src/config.ts","kind":"text"},…]}
                                                              (domain denial → isError, not transport error)

Denials: unauthenticated → isError vcs.unauthenticated; read-class token on a write tool → isError vcs.forbidden.
```

Deliberately absent from the MCP set (per FR13.1) — the full absence list: `vcs_repo_enable` (admin-tier), `vcs_ref_update` (ref moves go through `ox api vcs.ref_update`/raw RPC or the CLI), `vcs_changeset_get`, `vcs_git_import`, `vcs_git_export`, and any checkout tool (materialization is a client-side disk operation, FR8). This is the pinned surface, not an omission — `vcs_tree` covers manifest inspection for want-based fetch; whether `vcs_changeset_get` also belongs on MCP is a contract-design call (parked question in memory.md).

## S4 — JSON-RPC envelopes + object routes (Kit, US8.1)

Request bodies use the real `RpcRequest` schema — `{"procedure": <proc>, "input": <args>}` (`oxidean-core` `RpcRequest`; `docs/API.md`).

```
POST /api/rpc {"procedure":"vcs.log","input":{"owner":"myorg","name":"backend","from":"main","limit":50}}
→ 200 {"ok":true,"data":{"entries":[{"id":"cs_01M4…","parents":["cs_01K…"],"author":"ava-agent","message":"…","ts":"2026-10-18T05:12:00Z"}],"cursor":"cur_eyJ…"}}

POST /api/rpc {"procedure":"vcs.ref_update","input":{"owner":"myorg","name":"backend","ref":"main","expected_old":"cs_01H…","new":"cs_01M4…"}}
→ 200 {"ok":false,"error":{"code":"vcs.ref_conflict","message":"ref \"main\" is at cs_01J7…","data":{"actual":"cs_01J7…"}}}
   (note: the ref is carried as `ref`, matching the `#[serde(rename="ref")]` repo-arg precedent — repo `name` and ref `name` never collide in one input)

GET /api/vcs/myorg/backend/objects/sha256:7f3a…        → 200 octet-stream, Cache-Control: immutable, max-age=31536000
GET …/objects/sha256:dead…                             → 404 {"ok":false,"error":{"code":"vcs.not_found"}}
POST /api/vcs/myorg/backend/objects  (body = bytes)    → 200 {"ok":true,"data":{"hash":"sha256:7f3a…"}}
POST … on disabled/never-enabled repo                  → 403 {"ok":false,"error":{"code":"vcs.disabled"}}
POST … exceeding size limit                            → 413 {"ok":false,"error":{"code":"vcs.quota_exceeded"}}
Cookie-only request on any /api/vcs/… route            → 401 typed denial (never authorized)
```

Object routes return real HTTP statuses (mixed-media: bytes on success, JSON envelope on failure); RPC keeps the single-200 + typed-envelope convention.

## S5 — Activity feed entries (Ravi, US2.7)

Repo activity rows render in the existing feed with `vcs.*` kinds (allow-list extension per AC2.7.3), attributed to the authenticated identity — e.g. `ava-agent created changeset cs_01M4…`, `ava-agent moved ref agent/task-77`. One row per op; no per-object rows; machine bursts distinguishable by actor/kind for filtering.

## Coverage notes

- S1 covers US1.1–US1.3 states (incl. archived/read-only and `db.not_configured` error text).
- S2 renders enable/disable/status (S2.1); commit incl. root-commit bootstrap, inline + PUT blob transports, rename/exec modes, idempotent resubmit, same-parent fork (S2.2); the full US4.x read set — log, tree, diff, blame, path_history, merge_base, file_get (`cat`), changeset_get (`show`) — with pagination flags (S2.3); CAS create/update/delete + conflict (S2.4); merge + rebase (S2.5); sparse + cached checkout with attribute fidelity (S2.6); import/export incl. lookup and protection denial (S2.7).
- S3 covers the 12-tool MCP surface + `isError` discipline (AC8.3.2) and enumerates the deliberate absences.
- S4 covers AC8.1.1 envelope, AC8.1.4 taxonomy, FR11 object routes with pinned HTTP statuses, AC8.5.1 cookie denial.
- US8.2 registration and AC1.2.2 `db.not_configured` are structural/runtime, not visual — covered by the error rows in the state/envelope tables.

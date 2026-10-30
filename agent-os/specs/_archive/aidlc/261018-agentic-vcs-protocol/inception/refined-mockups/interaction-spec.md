# Interaction Specification — Side-by-Side Agentic VCS Protocol

Per-surface interaction contracts. Format follows `component-spec-template.md` where the surface is a visual component; machine surfaces use the same states/props discipline adapted to their transport.

## C1 — `VcsToggle` (Octane `.tsrx`, repo settings)

| Field | Value |
|---|---|
| Component | `VcsToggle` — a labelled checkbox inside a new "Source control protocols" section of `[owner]/[repo]/settings` |
| Description | Enables/disables the per-repo VCS surface via `vcs.repo_enable` |
| Category | input |

### States

| State | Description | Trigger |
|---|---|---|
| loading | checkbox indeterminate/disabled until flag resolves | `vcs.status` read in flight on page load (TanStack Query `isLoading`) |
| never-enabled | renders as unchecked-default (NOT error) | `vcs.status` returns `vcs.disabled` — every repo's starting state |
| default | checkbox reflects stored flag | `vcs.status` resolved |
| disabled-input | non-interactive, muted | repo archived, or caller lacks admin class |
| saving | control + Save button inert, "Saving…" | mutation in flight |
| error | inline red error under control, control reverts to stored value | `vcs.repo_enable` denial (`vcs.forbidden`, `db.not_configured`, network) |
| success | control shows new value, transient "Saved" confirmation | mutation committed |

### Props / Inputs

| Prop | Type | Required | Default | Description |
|---|---|---|---|---|
| owner | string | yes | — | repo owner segment |
| repo | string | yes | — | repo name |
| enabled | boolean | yes | — | current stored flag (from `vcs.status`) |
| canAdmin | boolean | yes | — | admin/owner permission on the repo (AC1.1.1, AC8.6.1) |
| archived | boolean | yes | false | disables the control when true (AC1.3.3) |

### Responsive Behaviour

Inherits the settings page layout — single-column stacked section at all breakpoints; no bespoke responsive rules.

### Accessibility

| Requirement | Implementation |
|---|---|
| ARIA role | native `checkbox` inside a `fieldset`/`legend` "Source control protocols" |
| Keyboard interaction | Tab to focus; Space toggles; Enter submits the section's Save |
| Label / aria-label | visible label "Enable the VCS protocol" + description paragraph (labelled-by) |
| Contrast ratio | WCAG AA; error text uses the existing destructive text color token |
| Screen reader | state change announced via the Save result; errors use `role="alert"` inline text |
| Focus management | after save, focus stays on the control; on error, focus moves to the error text |

### Usage Example

```tsrx
// inside [owner]/[repo]/settings page — TanStack Query mutation → vcs.repo_enable
function VcsToggle() @{
  // useQuery(vcs.status) for enabled; useMutation(vcs.repo_enable {enabled})
@}
```

---

## C2 — `ox vcs` command family (CLI)

| Field | Value |
|---|---|
| Component | `ox vcs <subcommand>` — 16 command names per AC8.4.1 (`show` and `cat` are DISTINCT: `show` → `vcs.changeset_get`, `cat` → `vcs.file_get`) |
| Description | Human + script interface to the `vcs.*` RPC family |
| Category | input (machine transport) |

### States (per invocation)

| State | Description | Trigger |
|---|---|---|
| success | human text on stdout; `{ok:true,data}` under `--json`; exit 0 | procedure ok |
| empty result | explicit zero-state line (`no changesets` / `[]`); `{ok:true,…}` | log/ref-list/status on fresh repo |
| typed-domain-error | `ox: <code>: <message>` on stderr; `{ok:false,error:{code,message,data?}}` on stderr under `--json`; non-zero exit | `vcs.*`/`db.*` error envelope |
| auth-failure | `ox: vcs.unauthenticated:`/`vcs.forbidden:` on stderr; exit 3 (auth tier, beside `auth.*`) | bad/insufficient PAT |
| client-side failure | `ox: <local-code>: <message>` on stderr; synthesized `{ok:false}` under `--json`; exit 1 | checkout target conflict (AC6.1.3), cache corruption — local errors, not `vcs.*` codes |
| transport-failure | `ox: <transport>: <message>` on stderr; non-zero | unreachable, TLS |

### Inputs

| Input | Shape | Notes |
|---|---|---|
| subcommand | positional | `enable status commit log diff show cat blame history merge-base merge rebase checkout ref import export` — `show` = `vcs.changeset_get` (manifest), `cat` = `vcs.file_get` (file bytes at a changeset); distinct commands, not aliases |
| `--json` | flag | prints RPC envelope verbatim via `Out::Rpc {envelope, pretty}` (NOT `Out::Report` — that local variant always exits 0); error envelopes go to stderr |
| `--limit` / `--cursor` / `--all` | flags on `log`, `tree`, `history`, `blame` | client-side pagination per AC4.1.2; `--all` pages to the end and warns it may take many requests |
| `--paths` | filter on `checkout`/`diff`-shaped calls | client-side sparse selection (AC3.4.1) |
| `--change` | repeatable `path:kind:mode:blobspec` | `kind` ∈ add\|modify\|delete\|rename (rename carries `old>new`); `mode` = `100644`/`100755`/`120000`; `blobspec` = `@sha256:…` (prior PUT) or `@file:<path>` (inline contents — AC2.1.2's second transport field) |
| `--expect` / `--to` | CAS args on `ref update` | `--expect-absent` for create; `--delete` for delete (null target) |
| `--force` | flag on `export` | overrides fast-forward requirement (AC7.2.2), never protection verdicts |
| `--to` | arg on `export` | target Git branch receiving the export move |
| `--lookup` | flag on `import` | `git_oid → changeset_id` query (AC7.1.1) |

### Accessibility analogue (terminal)

- No color-only semantics — status/conflict always spelled in text; color is decoration only.
- Stable single-line summary + optional table so screen readers and `| jq` pipelines parse deterministically.
- Errors on stderr; results on stdout; exit codes never overloaded (0 = ok, non-zero = typed failure).

---

## C3 — MCP tools (`vcs_*`, 12 tools)

| Field | Value |
|---|---|
| Component | MCP `tools/call` descriptors emitted by `oxidean-vcs` (name/description/input-schema), gated by `oxidean-api::mcp` `Access` |
| Description | Agent-facing protocol surface mirroring the core procedures |
| Category | input (agent transport) |

### States

| State | Description | Trigger |
|---|---|---|
| success | tool result content = procedure's `data` payload | procedure ok |
| domain-denial | `isError` content carrying `{code, message}` | `vcs.*` typed error, `vcs.forbidden` on read-class token, `OXIDEAN_MCP_ENABLED` off, `admin.mcp.*` denial |
| transport | MCP-layer error (not tool content) | transport/auth handshake |

### Tool → Access mapping

| Tool | Access class |
|---|---|
| `vcs_status` `vcs_log` `vcs_diff` `vcs_tree` `vcs_file_get` `vcs_blame` `vcs_path_history` `vcs_merge_base` `vcs_ref_list` | `RepoRead` |
| `vcs_changeset_create` `vcs_merge` `vcs_rebase` | `RepoWrite` |

(Full absence list per FR13.1: `vcs_repo_enable` (admin-tier — flows through `vcs.repo_enable` RPC/`ox vcs enable`/the settings toggle), `vcs_ref_update` (ref moves go through `ox api vcs.ref_update`/raw RPC or the CLI), `vcs_changeset_get`, `vcs_git_import`, `vcs_git_export`, and any checkout tool — materialization is a client-side disk operation per FR8.)

Tool args carry separate `owner` and `name` string fields — the repo-target convention the MCP `fg_repo_access_check` reads; a combined `repo` field would fail open past the grant check.

---

## C4 — `/api/rpc` envelope + `/api/vcs/…` object routes

| Field | Value |
|---|---|
| Component | typed JSON-RPC procedures + per-object HTTP routes |
| Description | the wire contract every consumer shares |
| Category | input (machine transport) |

### Envelope states

| State | Shape | Notes |
|---|---|---|
| ok | `{"ok":true,"data":{…}}` | per-procedure `data` schema generated into `@oxidean/api-client` |
| domain error | `{"ok":false,"error":{"code":"vcs.*","message":"…","data":{…}?}}` | codes from AC8.1.4 taxonomy; `data` carries typed detail (e.g. `ref_conflict` → `{actual}`; `conflict` → `{conflicts:[{path,kind}]}`) |
| auth | `{"ok":false,"error":{"code":"vcs.unauthenticated" \| "vcs.forbidden"}}` | cookie auth never authorizes `/api/vcs/*` |
| infra | `{"ok":false,"error":{"code":"db.not_configured"}}` | fail-closed `Database::skipped()` |

### Object-route states

| Route | Success | Errors |
|---|---|---|
| `GET /api/vcs/{o}/{r}/objects/{hash}` | `200` bytes, `Cache-Control: immutable, max-age=31536000` | `404` `{ok:false,code:"vcs.not_found"}`; `403` `vcs.disabled`/`vcs.forbidden`; `401` `vcs.unauthenticated` (cookie-only is always denied) |
| `POST /api/vcs/{o}/{r}/objects` | `200` `{"ok":true,"data":{"hash":"sha256:…"}}` (dedup no-op on re-upload) | `403` `vcs.disabled`/`vcs.forbidden`; `401` `vcs.unauthenticated`; `413` `vcs.quota_exceeded` (size limits) |

Object routes carry real HTTP statuses — mixed-media: bytes on GET success, JSON envelope on failure. RPC keeps the single-`200` + typed-envelope convention.

### Pagination contract

`vcs.log`, `vcs.tree`, `vcs.path_history`, `vcs.blame` accept `{limit, cursor}` and return `{entries, cursor?}` — `cursor` absent = end. `vcs.diff` boundedness is parked for contract-design (open question R-14 in the user-stories diary).

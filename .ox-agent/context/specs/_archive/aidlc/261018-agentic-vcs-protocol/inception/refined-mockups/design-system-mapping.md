# Design System Mapping — Side-by-Side Agentic VCS Protocol

## Scope

One Octane component enters the web UI (`VcsToggle`); everything else maps onto existing machine-surface idioms. No new visual system, no new dependency, no new design tokens.

## Web UI mapping (the single Octane surface)

| Design element | Existing convention | Where |
|---|---|---|
| Settings section | `apps/web/src/components/settings/` section pattern — `fieldset` + `legend` + description paragraph | `[owner]/[repo]/settings/index.astro` page |
| Checkbox control | existing labelled-checkbox affordance used across settings forms | same page |
| Save mutation | `@octanejs/tanstack-form`/`useMutation` → typed RPC call (`vcs.repo_enable`) via `@oxidean/api-client` | settings mutations precedent on the same page |
| Inline error text | existing destructive/error text color token + `role="alert"` | settings forms precedent |
| Loading/saving state | button `disabled` + "Saving…" label | settings forms precedent |
| Archived read-only | muted `disabled` control | existing archived-repo affordances |

Hard rules honored: Octane `.tsrx` authoring (not React JSX), TanStack Query for server state, no new state library, generated client only (never hand-edited), `make web-lint` + `make web-format-check` before commit.

## Machine-surface mappings (existing idioms, no new system)

| Surface element | Maps to | Evidence/anchor |
|---|---|---|
| `ox vcs` output | `Out::Rpc {envelope, pretty}` — `--json` prints the RPC envelope verbatim (errors to stderr), error → `rpc::error_exit_code` (auth tier 3, domain 1). `Out::Report` is the local-only variant that always exits 0 — never used for `vcs.*` calls | `crates/oxidean-cli/src/commands.rs` `Out::Rpc`, `rpc::error_exit_code` |
| CLI error text | `ox: <code>: <message>` on stderr, non-zero exit | existing `ox` error style (`main.rs`) |
| Wire repo-target args | separate `{owner, name}` fields — the shape RPC inputs, MCP tool schemas, and the PAT/FG-grant extractors all read | `repo_types.rs`, `mcp.rs`, `proc_classes.rs` conventions |
| RPC envelope | `{"ok":true,"data"}` / `{"ok":false,"error":{"code","message","data"?}}` | existing `/api/rpc` envelope; `AppError` path |
| Error codes | `vcs.*`/`db.*` typed codes from the AC8.1.4 taxonomy | requirements FR10/NFR8 |
| Object-route auth | PAT/OAT only, sha256 lookup + `FailedAuthLimiter`, cookie never authorizes | `mcp.rs` PAT dispatch precedent |
| Activity feed rows | `repo_activity` via `Database::insert_repo_activity`; `vcs.*` kinds extending `VALID_PUSH_TYPES` | `repo/activity.rs` precedent |
| MCP gating | `Access` classes (`RepoRead`/`RepoWrite`) + `OXIDEAN_MCP_ENABLED`/`admin.mcp.*`; domain denials → `isError` | `oxidean-api::mcp` |
| Pagination | `{limit, cursor}` in, `{entries, cursor?}` out | existing paginated RPC precedent |

## Nothing new introduced

- No new CSS tokens, fonts, or components beyond `VcsToggle` reusing existing settings patterns.
- No new CLI output format — `Out::Rpc` and the `{ok}` envelope already cover every state.
- No new auth mechanism — PAT/OAT + repo ACLs reused end-to-end.

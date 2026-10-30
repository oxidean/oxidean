# `ox` ↔ `gh` parity matrix

`ox` deliberately tracks the GitHub CLI's command surface where the Oxidean
API can back it. This matrix is **pinned to `gh` 2.96.0** (2026-07-02) — the
version `gh --version` reported when the matrix was last reconciled.

Statuses:

- **implemented** — the command exists and covers the everyday use
- **partial** — the group exists; listed subcommands/flags are missing
- **planned** — tracked work, not yet implemented
- **not applicable** — GitHub-only surface or no Oxidean server capability
- **ox-only** — `ox` ships surface `gh` does not have

`make cli-parity-check` (`scripts/check-cli-parity.sh`) re-derives the `gh`
surface and fails when an `implemented`/`partial` row drifts.

## Command groups

| gh 2.96.0 | ox | status | gap notes |
|---|---|---|---|
| `auth login` | `auth login` (+`--web`, `--scopes`) | implemented | ox adds `--scopes` scope picker |
| `auth logout` | `auth logout` | implemented | |
| `auth refresh` | `auth refresh` (`--scopes`, `--keep-scopes`, `--reset-scopes`) | implemented | ox mints via device grant + self-revokes old token; defaults to the full first-party scope set |
| `auth setup-git` | `auth setup-git` (+`git-credential`) | implemented | |
| `auth status` | `auth status` | implemented | shows token name/scopes/expiry via `pat.self`; warns when scopes leave procedures uncovered |
| `auth switch` | `auth switch` | implemented | |
| `auth token` | `auth token` | implemented | |
| — | `auth revoke` | ox-only | self-revoke current PAT + logout |
| `browse` | `browse` | partial | `--settings`, `--projects`, `--wiki`, commit/branch targets not mapped |
| `codespace` (13 subs) | — | not applicable | no codespaces concept |
| `gist` (7 subs) | — | not applicable | no gists |
| `issue create` | `issue create` | implemented | |
| `issue list` | `issue list` | implemented | |
| `issue status` | — | planned | personal dashboard across repos |
| `issue view` | `issue view` | implemented | markdown-rendered body on TTY |
| `issue close` / `reopen` | same | implemented | |
| `issue comment` | `issue comment` | implemented | |
| `issue delete` | `issue delete` | implemented | |
| `issue edit` | `issue edit` | implemented | |
| `issue develop` | — | planned | linked branches |
| `issue lock` / `unlock` | — | planned | needs moderation API |
| `issue pin` / `unpin` | — | planned | |
| `issue transfer` | — | planned | needs `issue.transfer` RPC |
| `org list` | `org list` | implemented | |
| — | `org view|create|members|invites` | ox-only | gh has no equivalent |
| `pr create` | `pr create` | implemented | |
| `pr list` | `pr list` | implemented | |
| `pr status` | — | planned | |
| `pr view` | `pr view` | implemented | markdown-rendered body |
| `pr checkout` | `pr checkout` | implemented | |
| `pr checks` | `pr checks` | implemented | |
| `pr close` / `reopen` | same | implemented | |
| `pr comment` | `pr comment` | implemented | |
| `pr diff` | `pr diff` | implemented | |
| `pr edit` | — | planned | needs `pull.update` coverage |
| `pr lock` / `unlock` | — | planned | |
| `pr merge` | `pr merge` | implemented | |
| `pr ready` | `pr ready` | implemented | |
| `pr revert` | — | planned | needs revert RPC |
| `pr review` | `pr review` | implemented | |
| `pr update-branch` | `pr update-branch` | implemented | |
| — | `pr files` | ox-only | files-changed list |
| `project` (19 subs) | — | not applicable | `project`/`read:project` scopes reserved; boards not shipped |
| `release create` | `release create` | implemented | |
| `release list` | `release list` | implemented | |
| `release view` | `release view` | implemented | markdown-rendered notes |
| `release edit` | `release edit` | implemented | |
| `release delete` | `release delete` | implemented | |
| `release upload` | `release upload` | implemented | |
| `release download` | `release download` | implemented | |
| `release delete-asset` | `release delete-asset` | implemented | |
| `release verify` / `verify-asset` | — | planned | checksum/signature surface |
| `repo create` | `repo create` | implemented | `--template <ref>` maps builtin stack ids, instance-pack slugs, and `owner/name` template repos onto `stack_id`; bare on a TTY runs a wizard |
| `repo list` | `repo list` | implemented | |
| `repo view` | `repo view` | implemented | |
| `repo fork` | `repo fork` | implemented | |
| `repo sync` | `repo sync` | implemented | |
| `repo clone` | `repo clone` | implemented | |
| `repo edit` | `repo edit` | implemented | |
| `repo rename` | `repo rename` | implemented | |
| `repo archive` / `unarchive` | same | implemented | |
| `repo delete` | `repo delete` | implemented | |
| `repo deploy-key` | `repo deploy-key` | implemented | |
| `repo set-default` | — | planned | |
| `repo autolink` | — | planned | needs autolink API |
| `repo gitignore` / `license` | — | partial | `ox repo templates` lists the gitignore catalog + stacks; dedicated list subs planned |
| `repo read-dir` / `read-file` | — | planned | via `ox api` today |
| — | `repo transfer` | ox-only | |
| — | `repo star|unstar|watch|unwatch` | ox-only | gh shells out to API instead |
| — | `repo collaborator|invites` | ox-only | |
| — | `repo branch-protection|tag-protection` | ox-only | gh's `ruleset` is the loose analog |
| — | `repo mirror|merge-settings|lfs|quota` | ox-only | Oxidean-specific plumbing |
| `ruleset check/list/view` | — | partial | `repo branch-protection`/`tag-protection` cover rule authoring, not eval |
| `run list` / `view` / `watch` / `rerun` / `cancel` | same | implemented | `--job`/`--log` for logs |
| `run delete` | — | planned | |
| `run download` | — | planned | artifact download |
| `secret list` / `set` / `delete` | same | implemented | |
| `variable list/get/set/delete` | — | planned | Actions variables |
| `cache list` / `delete` | — | planned | Actions cache |
| `workflow list` / `view` / `run` | same | implemented | |
| `workflow disable` / `enable` | — | planned | |
| — | `workflow template list` / `view` | ox-only | starter-workflow catalog (builtin + instance + org `.github` repo); gh has no equivalent subcommand |
| — | `workflow create` | ox-only | drafts a workflow file onto a branch, a new PR, or an open PR's head |
| `ssh-key list` / `add` / `delete` | same | implemented | |
| `gpg-key list` / `add` / `delete` | same | implemented | |
| `label list` / `create` / `edit` / `delete` | same | implemented | |
| `label clone` | — | planned | |
| `api` | `api` | implemented | **divergence**: `ox api <proc> -f k=v -F k=<json>` vs gh's `--field`; `ox` calls typed RPC, not REST |
| `search repos` | `search --type repos` | implemented | |
| `search issues` / `prs` | `search --type issues|pulls` | implemented | |
| `search code` / `commits` | — | planned | needs search backend support |
| `status` | — | planned | cross-repo dashboard |
| `alias` (4 subs) | — | not applicable | ox commands dispatch through a static table; documented divergence |
| `config get` / `list` / `set` | `config get|list|set` | implemented | |
| `config clear-cache` | — | not applicable | no local response cache |
| `completion` | `completion bash|zsh|fish|pwsh` | implemented | |
| `extension` (8 subs) | — | not applicable | `ox skill` + `ox mcp` fill the agent-integration niche instead |
| `attestation` (3 subs) | — | not applicable | |
| `discussion` (5 subs) | — | not applicable | no discussions |
| `licenses` | — | not applicable | |
| `copilot` / `agent-task` / `preview` / `co` | — | not applicable | GitHub-hosted agent surfaces |
| — | `pkg list` | ox-only | package registry |
| — | `notification list|read|unread` | ox-only | gh has no notification group |
| — | `webhook …` (7 subs) | ox-only | |
| — | `email …` (5 subs) | ox-only | |
| — | `repo templates` | ox-only | template catalog (builtin packs, instance packs, template repos) |
| — | `self-update` | ox-only | instance-served upgrades |
| — | `skill print|install`, `mcp …` | ox-only | agent/harness integration |
| — | `pilot pair|run|disconnect|status` | ox-only | implemented — oxpilot computer-use connector (C3 routes, not /api/rpc) |
| — | `status create` | ox-only | commit status API |
| — | `vcs …` (16 subs) | ox-only | side-by-side agentic source control — immutable changesets, CAS refs, git import/export |

## Terminal / UX behavior

| gh behavior | ox | status |
|---|---|---|
| Prompts for missing values on TTY; flags override | flag wins → TTY prompt → usage error headless | implemented (`auth login`; prompt helpers shared) |
| Pager on long output (`GH_PAGER`/`PAGER`) | `OXIDEAN_PAGER` → `PAGER` → `less -FRX` | implemented |
| `NO_COLOR`/`CLICOLOR`/`GH_FORCE_TTY` | `NO_COLOR`/`CLICOLOR`/`OXIDEAN_FORCE_TTY` | implemented (anstream) |
| Spinner on waits, progress on uploads | spinner on device-grant poll; progress helper | implemented |
| `--json <fields>` + `--jq` + `--template` | `--json` prints raw envelope; `--jq` (jaq) filters `data` | partial — `--json` field selection and `--template` are **documented divergences** |
| OS keychain for credentials | OS keyring via `keyring`, `config.json` fallback headless | implemented (`secret_store` per instance) |
| `$EDITOR` compose for bodies | — | planned |
| OSC-8 hyperlinks | enabled on TTY | implemented |
| Markdown rendering in `view` | issue/pr/release bodies via termimad on TTY | implemented |
| Exit codes (`gh help exit-codes`) | 0 ok · 1 error · 2 usage · 3 auth | implemented (`auth.*` → 3 + hints) |

## Documented divergences

- **`--json` semantics** — gh's `--json` takes a field list and shapes the
  object; `ox --json` always prints the full RPC envelope `{ok,data|error}`.
  Use `--jq <filter>` for projection (applied to `data`).
- **No `ox pat` group** — PAT management lives in the web UI and
  `admin.pat.*` RPCs. The CLI manages only its own credential:
  `auth status`/`refresh`/`revoke`. Deliberate: a token should not mint or
  list tokens other than itself.
- **No `ox admin` group** — `admin.*` RPCs are session-cookie only, so a
  PAT-authenticated CLI cannot call them at all; administration lives in
  the web UI (**Admin → …**). Same for `ox invite accept` — invite
  acceptance is a web signup flow.
- **`ox api` calls typed RPC** — `ox api repo.get -f owner=o -f name=n`, not
  REST paths. `-f` sends strings, `-F` sends typed JSON values.
- **Repo detection** — positional `<owner/name>` → `-R/--repo` →
  `OXIDEAN_REPO` → git remote (`upstream`, `origin`, `fork`; HTTPS, SCP-like,
  and `ssh://` URLs).
- **Token expiry** — `auth login --web` tokens expire per instance policy
  (`OXIDEAN_PAT_*` env / **Admin → Auth settings**), unlike gh's
  non-expiring OAuth token. `auth refresh` is the renewal path.

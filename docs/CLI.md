# `ox` — Oxidean CLI

`ox` is the companion command-line tool for Oxidean instances. It speaks the
same typed JSON-RPC surface as the web app (`POST {instance}/api/rpc`, see
[API.md](API.md)), works against any instance URL, and is designed to be
scriptable: every command accepts `--json` to print the raw RPC response
envelope.

The binary lives at `crates/oxidean-cli` in the Cargo workspace.

## Install

Any Oxidean instance that ships the CLI distribution serves install scripts
— also linked under **Settings → Command line**:

```bash
# Linux / macOS / FreeBSD (sh — musl systems like Alpine get the musl build)
curl -fsSL https://forge.example.com/cli/install.sh | sh
```

```powershell
# Windows (PowerShell 5.1+)
irm https://forge.example.com/cli/install.ps1 | iex
```

The scripts detect your platform, download `ox-{target}` plus its published
`.sha256`, verify the checksum, and install to `~/.local/bin/ox`
(`%LOCALAPPDATA%\Programs\ox` on Windows — override either with
`OXIDEAN_INSTALL_DIR`). Supported targets: `x86_64`/`aarch64` for Linux (gnu
and musl), macOS, and Windows; plus `armv7-unknown-linux-gnueabihf` and
`x86_64-unknown-freebsd` where the instance ships them.

The binary/checksum base defaults to `{instance}/cli/bin`; point
`OXIDEAN_BIN_BASE` at any flat mirror instead — e.g. a GitHub release's
download URL, whose `install.sh`/`install.ps1` assets are pre-rendered for
standalone installs:

```bash
DL=https://github.com/oxidean/oxidean/releases/download/vX.Y.Z
curl -fsSL "$DL/install.sh" | sh          # already pinned to that release
OXIDEAN_BIN_BASE="$DL" sh install.sh      # or: template + explicit mirror
```

Tagged releases also publish native packages — `ox-*.deb` (apt),
`ox-*.rpm` (dnf/zypper), `ox-*.apk` (apk), `ox-*.pkg.tar.zst` (pacman) —
each `ox` binary installed to `/usr/bin/ox`:

```bash
sudo dpkg -i ox-*-amd64.deb            # Debian/Ubuntu
sudo dnf install ./ox-*-x86_64.rpm     # Fedora/RHEL
apk add --allow-untrusted ox-*.apk     # Alpine
sudo pacman -U ox-*.pkg.tar.zst        # Arch
```

If the instance has no prebuilt binary for your platform, install from source:

```bash
cargo build --release -p oxidean-cli
# binary lands at target/release/ox
cargo install --path crates/oxidean-cli   # optional: installs `ox`
```

## Updating

`ox self-update` replaces this binary with **the build the configured
instance currently stages** under `/cli/bin` — not a global channel:

```bash
ox self-update
```

It fetches `{instance}/cli/latest`, compares the advertised workspace
`version` **and** the sha256 for your platform to the running executable,
downloads `ox-{your-target}` when either differs, verifies the checksum, and
swaps in place. Same semver with a rebuilt matrix still refreshes the binary.
When the instance has no artifact for your platform it says so and points at
the installer instead.

RPC compatibility is separate: `system.manifest` (`min_cli_version`,
procedures, protocol) — see [Instance compatibility](#instance-compatibility-capability-manifest)
below. Dist lockstep is “match this instance’s staged bytes.”

After a successful swap, `ox` refreshes any installed agent skills — every
existing `ox/SKILL.md` it finds under project or global harness dirs is
rewritten from the new binary's embedded copy (the same sweep as
`ox skill install --upgrade`; no new installs are created and refresh
failures only warn). Pass `--no-skill-refresh` to skip that step.

### For operators: serving binaries

The API serves every `ox-{rust-target-triple}` file found in
`OXIDEAN_CLI_DIST_DIR` (default `/usr/local/share/oxidean-cli`). At image
build time the Dockerfile always cargo-builds the server's own platform —
with the default **empty** `OXIDEAN_CLI_DIST_URL`, that host binary is all a
deploy serves, and it is always the same commit the image was built from.
When `OXIDEAN_CLI_DIST_URL` is **non-empty**, the build fetches the full flat
matrix (`ox-targets.txt` + every `ox-{triple}` + `.sha256`, plus
`SOURCE_COMMIT`) and **overwrites** the host triple from that feed so staged
digests match the release (required by the ox CLI image-matrix job); the
fetch is **fail-closed** (retries, then fail the image build).
`OXIDEAN_CLI_DIST_BUST` is a Docker build-arg that invalidates only the
matrix-fetch layer so staging can re-pull `ox-dist` after a same-commit
redeploy. `OXIDEAN_SOURCE_COMMIT` records the built commit into
`SOURCE_COMMIT` on host-only builds — Railway injects
`RAILWAY_GIT_COMMIT_SHA` automatically, Compose/self-hosts can pass
`OXIDEAN_SOURCE_COMMIT=$(git rev-parse HEAD)`. Extra targets can be dropped
into the dist dir by hand:

```text
/usr/local/share/oxidean-cli/
  ox-x86_64-unknown-linux-gnu
  ox-aarch64-apple-darwin
  ox-x86_64-pc-windows-msvc
  SOURCE_COMMIT          # git SHA (feed's when URL set; OXIDEAN_SOURCE_COMMIT
                         # or RAILWAY_GIT_COMMIT_SHA on host-only builds)
```

`GET /cli/latest` reports the API package `version`, each target’s
sha256/size, and `dist_commit` when `SOURCE_COMMIT` is present.
`/cli/bin/{file}` serves artifacts unauthenticated so installs work before
`ox auth login` exists.

#### Oxidean Cloud (Railway / app.oxidean.dev)

Cloud IaC sets `OXIDEAN_CLI_DIST_URL` to this product’s rolling release
`https://github.com/oxidean/oxidean/releases/download/ox-dist` on **staging
and production only**, so those API images stage the full matrix:

1. `.github/workflows/ox-cli.yml` refreshes **`ox-dist`** on every successful
   run on `main` (and on `v*` tags), including `SOURCE_COMMIT`.
2. The job upserts `OXIDEAN_CLI_DIST_BUST=<sha>` on staging `api`, then
   redeploys (`scripts/railway-staging-api-redeploy.sh`) so the fetch layer
   re-runs even when the git SHA is unchanged.
3. Production promote rebuilds `api` with the same Cloud build-arg.

**Preview / PR environments leave `OXIDEAN_CLI_DIST_URL` unset** — they stage
the just-built host `ox`, so a PR's `/cli/bin` serves that PR's own CLI and
`GET /cli/latest` reports the built commit (`RAILWAY_GIT_COMMIT_SHA` →
`SOURCE_COMMIT`). Trade-off: preview serves linux-amd64 only; verify
darwin/windows binaries against staging or the `ox-dist` release.

Confirm with `GET /cli/latest` (darwin/windows listed) before asking users to
re-run the installer.

#### Self-hosted

Compose passes an empty build-arg (host-only, same as the Dockerfile
default). Point `--build-arg OXIDEAN_CLI_DIST_URL=` at **your** flat artifact
feed (same layout as `ox-dist`) if you publish a multi-arch matrix for your
fork/deployment. Dropping files into `OXIDEAN_CLI_DIST_DIR` at runtime also
works.

> **Distribution polish backlog:** macOS binaries ship unsigned today —
> Gatekeeper will flag them on first run (`xattr -d com.apple.quarantine` or
> System Settings → Privacy & Security → Open Anyway). Apple signing +
> notarization is tracked as a follow-up.

## Authentication

`ox` authenticates with a personal access token sent as
`Authorization: Bearer <pat>` on each RPC call. Create a PAT in the web UI
under **Settings → Personal access tokens**.

```bash
ox auth login --instance https://forge.example.com --token oxidean_pat_…

# CI-friendly: the token comes from the environment or stdin, never a prompt
OXIDEAN_TOKEN=oxidean_pat_… ox auth login --instance https://forge.example.com
echo "$TOKEN" | ox auth login --instance https://forge.example.com
echo "$TOKEN" | ox auth login --instance https://forge.example.com --token-stdin

# Browser device grant (RFC 8628): prints a user code + verification URL,
# approves in the browser at /oauth/device, polls until approved — the
# resulting PAT is stored like a --token login.
ox auth login --instance https://forge.example.com --web

# Pick the scopes the minted token carries (comma- or space-separated; the
# default covers the full CLI surface). Scopes come from the same catalog
# the web token form and `pat.catalog` expose.
ox auth login --web --scopes "repo workflow read:org"
```

On a TTY with no flags, `auth login` asks whether to use the browser flow or
paste a token — flag-free scripts still need `--token`, `--token-stdin`,
piped stdin, or `OXIDEAN_TOKEN`.

Credentials are stored through the OS keyring when a backend is available
(secret-service on Linux, Keychain on macOS, Credential Manager on Windows),
falling back to `config.json` under the config directory
(`$OXIDEAN_CONFIG_DIR`, else `$XDG_CONFIG_HOME/ox`, else `~/.config/ox`),
recorded with `0600` permissions. The config entry's `secret_store` field
records which store holds each instance's token. The instance becomes the
default for all subsequent commands.

```bash
ox auth status    # instance + username + token name/scopes/expiry (pat.self), exit 3 if unauthenticated
ox auth refresh   # mint a fresh token via the device grant and self-revoke the old one
                  #   [--scopes list] [--reset-scopes]
ox auth revoke    # revoke the stored token on the server and log out
ox auth switch    # pick a different default instance (picker, or pass the URL)
ox auth setup-git # register `ox auth git-credential` as the git credential helper
ox auth logout    # drop the stored token for the resolved instance
ox auth token     # print the resolved token (for scripts)
```

`refresh` reuses the old token's scopes unless `--scopes`/`--reset-scopes`
overrides them, persists the new credential only after it verifies, and
self-revokes the old one. `OXIDEAN_TOKEN`-supplied credentials can't be
refreshed or revoked here — manage those where they were issued.

> **Token expiry:** CLI-minted tokens follow the instance PAT policy
> (`OXIDEAN_PAT_DEFAULT_TTL_DAYS` / `OXIDEAN_PAT_MAX_TTL_DAYS` /
> `OXIDEAN_PAT_ALLOW_NO_EXPIRY` env, overridable per-instance under
> **Admin → Auth settings → Personal access token policy**). Expired or
> revoked tokens fail with distinct `auth.token_expired` /
> `auth.token_revoked` errors — `ox` prints the recovery hint
> (`auth refresh`, or `auth login` again).

> **Instance requirement:** `/api/rpc` accepts `Authorization: Bearer <pat>`
> on API-02-era servers and later. On older instances that only resolve
> session cookies, `ox auth login` still stores the token (with a note that
> verification was skipped) and commands fail with `auth.unauthenticated`
> until the server is upgraded.

## Instance resolution

Every command resolves the target instance in this order:

1. `--instance <url>` flag
2. `OXIDEAN_INSTANCE` environment variable
3. `default_instance` in the config file (set by `ox auth login`)

The token is resolved separately: `OXIDEAN_TOKEN` env, then the keyring or
config-stored token for the resolved instance. `ox config` exposes the local
file:

```bash
ox config list                                   # config file + instances (tokens redacted)
ox config get default_instance
ox config set default_instance https://forge.example.com   # must be logged in
ox config set instances.https://forge.example.com.token oxidean_pat_…
```

`config get`/`list` never print stored tokens — they report `set`/`unset`.

### Repository resolution

Commands that take `<owner/name>` resolve it in this order:

1. positional `<owner/name>` argument
2. `-R`/`--repo <owner/name>`
3. `OXIDEAN_REPO` environment variable
4. the git remotes in the current directory — `upstream`, then `origin`,
   then `fork` (HTTPS, `git@host:owner/name`, and `ssh://` URLs all parse)

Inside a clone, `ox issue list` with no arguments just works.

## Agent skill

`ox` embeds the consumer-facing agent `SKILL.md` and installs it into agent
harness skill dirs. The registry follows the vercel-labs/skills table (~70
harnesses — `claude`, `cursor`, `codex`, `copilot`, `gemini`, `opencode`,
`continue`, `trae`, `qwen`, `grok`, `junie`, …) plus `generic`, which covers
any harness that reads `.agents/skills`:

```bash
ox skill print                                  # write SKILL.md to stdout
ox skill install                                # TTY picker (detected harnesses first)
ox skill install --harness claude,cursor --project
ox skill install --harness all --global
ox skill install --detected --project           # only harnesses found installed
ox skill install --upgrade                      # rewrite existing installs only
```

Install targets `<skills-dir>/ox/SKILL.md` per harness, deduplicating shared
dirs (many harnesses read `.agents/skills`). `--detected` selects by config
dirs, project markers, and binaries on `PATH`; combined with `--harness` it
intersects. `--upgrade` never creates installs and never prompts — sweep all
harnesses, or narrow with `--harness`/`--detected`/`--project`/`--global`.
The same document is served by the instance at `/cli/skill.md`.

## MCP server

Every Oxidean instance serves a Model Context Protocol endpoint at
`POST {instance}/api/mcp` (see [MCP.md](MCP.md)). `ox mcp` wires agent
harnesses to it — same harness-selection machinery as `ox skill`:

```bash
ox mcp print [--harness <id>]             # config block to stdout (token placeholder)
ox mcp install                            # TTY picker (detected harnesses marked)
ox mcp install --harness claude,cursor --project
ox mcp install --detected --global
ox mcp install --upgrade                  # refresh existing entries only (no prompt)
ox mcp status                             # local config state + endpoint probe
ox mcp remove --harness claude --project
```

`install` merges an `oxidean` entry into each harness's MCP config
(`.mcp.json`, `.vscode/mcp.json`, `~/.cursor/mcp.json`, …): sibling servers
and unrelated keys are preserved, re-runs update the entry in place,
unparsable files are never clobbered, and token-bearing files land `0600`.
Harness ids: `claude`, `cursor`, `vscode`, `windsurf`, `gemini`, `codex`,
`kiro`, `opencode`, `zed` (stdio-bridged through `mcp-remote`), `generic`.

Auth precedence: `--token` / `--token-stdin` → `--web` (RFC 8628 device
grant, same browser sign-in as `ox auth login --web`) → the stored `ox`
login for the instance → `--anonymous` (no header; public repos only).
Codex writes `bearer_token_env_var = "OXIDEAN_TOKEN"` instead of the token —
export it for the harness. `status` never prints tokens; it lists found
configs and POSTs `initialize`/`tools/list` to report
reachable/disabled/unauthorized.

## Commands

`ox` has per-command help — `ox <group> --help` and `ox <group> <sub> --help`
list exact flags. The full surface, roughly `gh`-shaped:

```bash
# Repositories
ox repo list [--owner someorg]          # yours, or an owner's
ox repo view|fork octo/demo             # fork: [--into-owner o] [--into-name n]
ox repo create <name> [--org o] [--visibility public|private]
                                      #   --template <ref>: builtin stack id, instance
                                      #   pack slug, or owner/name template repo
                                      #   bare on a TTY → wizard (name, owner, template, visibility)
ox repo templates                     # template catalog the instance offers
ox repo sync|clone octo/demo            # clone runs `git clone` locally
ox repo edit o/n [--description d] [--topics a,b] [--visibility v]
ox repo rename|archive|unarchive octo/demo
ox repo delete octo/demo --confirm demo # soft delete, name echoed back
ox repo transfer o/n dest-owner --confirm name
ox repo star|unstar|watch|unwatch octo/demo
ox repo deploy-key list|add|delete o/n  # add: --title t --key-file pub [-w]
ox repo collaborator list|add|set-role|remove o/n --permission read|write|admin
ox repo invites list|create|link|revoke o/n
ox repo branch-protection|tag-protection list|set|delete o/n --pattern glob
ox repo mirror get|set|sync|delete|gen-key|rotate-secret o/n
ox repo merge-settings o/n [--merge on|off --squash on|off --rebase on|off]
ox repo lfs o/n [--usage|--objects|--enable|--disable]
ox repo quota o/n [--set <bytes>]

# Issues
ox issue list octo/demo [--state s] [--author u] [--assignee u] [--label l]
                        [--search q] [--limit n]
ox issue view|create|edit octo/demo [<n>] # edit: --title --body --label --assignee
ox issue close|reopen|delete|comment octo/demo <n>

# Pull requests
ox pr list|view octo/demo [<n>]         # same filters as issue list
ox pr create o/n --title t --base main --head feat [--draft]
ox pr merge o/n <n> [--merge|--squash|--rebase] [--delete-branch]
ox pr diff|files|checks o/n <n>
ox pr review o/n <n> --approve|--request-changes|--comment [--body b]
ox pr review o/n <n> --request <user>|--unrequest <user>|--dismiss <id>|--list
ox pr comment|ready|update-branch|close|reopen|checkout o/n <n>

# Actions
ox run list o/n [--status s --event e --branch b --workflow w --limit n]
ox run view o/n <run-id> [--log] [--job <id>]
ox run watch o/n <run-id> [--interval s] [--timeout s] [--exit-status]
ox run rerun|cancel o/n <run-id> [--failed-only]
ox workflow list|view|run o/n [<workflow-id>] [--ref r]
ox secret list|set|delete o/n [<NAME>]  # set: --body v | --file path | stdin
ox status create o/n --sha s --context c --state pending|success|failure|error

# Releases & packages
ox release list|view|create|edit|delete o/n [<tag>]
ox release upload|download o/n <tag> [<file>...|--pattern g --dir d --clobber]
ox release delete-asset o/n <tag> <filename>
ox pkg list [--owner octo]

# People & orgs
ox org list|view|create|members|invites
ox user view|lookup|follow|unfollow <u> | ox user starred|watched
ox notification list|read [<id>...|--all]|unread
ox label list|create|edit|delete <o/n>|--org <slug>
ox webhook list|create|edit|delete|ping <o/n> | ox webhook deliveries|redeliver

# Account
ox ssh-key|gpg-key list|add|delete      # add: --title t --key-file path|- 
ox email list|add|remove|set-primary|resend
# PATs are minted/listed in the web UI (Settings → Personal access tokens);
# the CLI manages only its own credential — `auth status|refresh|revoke`.
# Instance admin lives in the web UI (admin.* RPCs are session-only and
# can't be called with PAT auth) — or via `ox api admin.<proc>` if you
# authenticate another way.

# Maintenance & local
ox self-update                          # swap this binary for the instance's build
                                        #   (--no-skill-refresh / --no-mcp-refresh skip the
                                        #   post-swap skill + MCP config refresh sweeps)
ox skill print|install                  # embedded agent SKILL.md → harness dirs
ox mcp print|install|status|remove      # wire /api/mcp into harness MCP configs
                                        #   (install --upgrade refreshes existing entries)
ox config list|get|set                  # local config file (tokens redacted)
ox completion bash|zsh|fish|pwsh        # static shell completion script
ox search <query> [--type repos|issues|pulls|users|orgs|commits|code]
ox browse [<o/n>] [--print]             # open the repo (or instance) in a browser

# Escape hatch — any procedure, raw input fields
ox api repo.get -f owner=octo -f name=demo
ox api issue.get -f owner=octo -f name=demo -F number=12
```

`-f` sends the value as a string; `-F` parses the value as JSON (use it for
numbers, booleans, arrays, objects). `owner/name` positionals may be replaced
with `-R owner/name` anywhere a repo is expected (see
[Repository resolution](#repository-resolution)). Content flags follow `gh`:
`--body` inline, `--body-file <path>` or `-` for stdin, and piped stdin fills
in automatically on verbs like `issue comment` and `secret set`.

Release assets are the one path off `/api/rpc`: `release upload` POSTs
multipart to `/api/repos/{o}/{n}/releases/{id}/assets` and `release download`
GETs `/api/releases/assets/{id}` — both accept `Authorization: Bearer <pat>`.

## Instance compatibility (capability manifest)

Before running an RPC-backed command — including `auth status` — `ox` fetches
`system.manifest` from the instance (public, like `system.health`) with a short
timeout. The manifest is the server's **RPC** compatibility contract: which
procedures its dispatch table knows, its protocol and server versions, optional
capabilities (`mcp`, `rest`, `oauth`, `cli_dist`), and the oldest supported
CLI version. It does not replace install/`self-update` lockstep to the
instance's staged `/cli/bin` bytes (see [Updating](#updating)).

- A procedure missing from `procedures` fails fast:
  `command not supported by instance (missing <procedure>)`, exit code 2 —
  instead of an `rpc.unknown_procedure` round trip.
- Instances that predate `system.manifest`, unreachable instances, and
  malformed manifests **fail open**: `ox` prints a note on stderr and runs the
  command anyway, so the compatibility layer itself never breaks a working
  setup.
- When `min_cli_version` is newer than the installed `ox`, or the instance
  speaks a newer RPC protocol, `ox` prints an upgrade warning on stderr.
  Notices never touch stdout, so `--json` pipelines stay clean.
- The manifest is fetched once per invocation; multi-procedure commands
  (`pr checks`) check all their calls against the same fetch.

## Output and scripting

Default output is human-readable text. On a terminal, long output pages
through `$OXIDEAN_PAGER`/`$PAGER` (default `less -FRX`), issue/PR/release
bodies render as markdown, and links are clickable. `--json` prints the raw
RPC envelope verbatim — `{ "ok": true, "data": … }` on success,
`{ "ok": false, "error": { "code", "message" } }` on failure — ready for
`jq`. The built-in `--jq <filter>` (jaq) applies a jq filter to `data`
directly, printing each result — strings bare, like `jq -r`:

```bash
ox repo list --json | jq '.data.repos[].name'
ox issue list octo/demo --state open --json \
  | jq -r '.data.issues[] | "\(.number)\t\(.title)"'
ox api repo.explore -F limit=5 --json | jq '.data'

# same result without an external jq:
ox repo list --jq '.repos[].name'
ox issue list octo/demo --jq '.issues[] | "\(.number)\t\(.title)"'
```

Terminal honors `NO_COLOR`/`CLICOLOR`/`CLICOLOR_FORCE`, with
`OXIDEAN_FORCE_TTY` to force TTY behavior; progress indicators write to
stderr only, so piped stdout stays byte-clean.

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | success |
| 1 | API or transport error (any `{ok:false}` envelope, unreachable host) |
| 2 | usage error (bad flags, missing arguments, no instance configured, command unsupported by the instance manifest) |
| 3 | auth error (`auth.unauthenticated`, `auth.token_expired`, `auth.token_revoked`, `auth.pat_scope`, HTTP 401/403, missing token) |

Auth failures print a recovery hint on stderr — expired/revoked tokens point
at `ox auth refresh`, missing scopes point at `ox auth status` and
`auth login --web --scopes`.

## Environment

| Variable | Purpose |
| --- | --- |
| `OXIDEAN_INSTANCE` | Default instance URL when `--instance` is absent |
| `OXIDEAN_TOKEN` | Token override for any command (wins over the stored token) |
| `OXIDEAN_CONFIG_DIR` | Config directory override (default `~/.config/ox`) |
| `OXIDEAN_REPO` | Default `<owner/name>` when no positional or `-R` is given |
| `OXIDEAN_PAGER` | Pager for long output (falls back to `PAGER`, then `less -FRX`) |
| `OXIDEAN_FORCE_TTY` | Force terminal behavior (color, links, pager) even when piped |

## Parity with `gh`

`ox` tracks the GitHub CLI's command surface where the Oxidean API can back
it — see [CLI-PARITY.md](CLI-PARITY.md) for the versioned matrix
(`gh` 2.96.0) and the documented divergences. `make cli-parity-check` is the
drift guard.

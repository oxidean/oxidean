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

`ox` can replace itself with the binary the configured instance serves — the
CLI version tracks the server version, so updating keeps them in lockstep:

```bash
ox self-update
```

The command fetches `{instance}/cli/latest`, compares the advertised version
to the running build, downloads `ox-{your-target}` and its `.sha256`, verifies
the checksum, and swaps the executable in place. When the instance has no
artifact for your platform it says so and points at the installer instead.

After a successful swap, `ox` refreshes any installed agent skills — every
existing `ox/SKILL.md` it finds under project or global harness dirs is
rewritten from the new binary's embedded copy (the same sweep as
`ox skill install --upgrade`; no new installs are created and refresh
failures only warn). Pass `--no-skill-refresh` to skip that step.

### For operators: serving binaries

The API serves every `ox-{rust-target-triple}` file found in
`OXIDEAN_CLI_DIST_DIR` (default `/usr/local/share/oxidean-cli`). At image
build time the Dockerfile stages the server's own platform, then — when
`OXIDEAN_CLI_DIST_URL` is non-empty — fetches the full matrix
(`ox-targets.txt` + per-target binaries and `.sha256` digests). The default
URL is the rolling GitHub Release
`https://github.com/oxidean/oxidean/releases/download/ox-dist`. Pass an empty
build-arg (`OXIDEAN_CLI_DIST_URL=`) for airgapped / host-only images. Fetch
is fail-soft (retries, then warn and keep host-only) so a missing release
never fails the image build. Extra targets can always be dropped into the
dist dir by hand:

```text
/usr/local/share/oxidean-cli/
  ox-x86_64-unknown-linux-gnu
  ox-aarch64-apple-darwin
  ox-x86_64-pc-windows-msvc
```

`GET /cli/latest` reports the version (the API package version) and the
sha256/size of every target found; `/cli/bin/{file}` serves artifacts and
digests unauthenticated so installs work before `ox auth login` exists.

#### Cloud operators (Railway / app.oxidean.dev)

The instance only serves what was staged into the API image. On Oxidean Cloud
that is automatic:

1. `.github/workflows/ox-cli.yml` refreshes the rolling **`ox-dist`** release
   on every successful run on `main` (and on `v*` tags).
2. The same job redeploys staging `api` so `/cli` picks up the new matrix
   (`scripts/railway-staging-api-redeploy.sh`).
3. Railway IaC and the Dockerfile default both point
   `OXIDEAN_CLI_DIST_URL` at
   `https://github.com/oxidean/oxidean/releases/download/ox-dist`.
4. Production promote rebuilds `api` with that URL — no separate tag ritual.

Confirm with `GET /cli/latest` (should list `aarch64-apple-darwin` and the
rest of the matrix) before asking users to re-run the installer.

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
```

Login writes `config.json` under the config directory
(`$OXIDEAN_CONFIG_DIR`, else `$XDG_CONFIG_HOME/ox`, else `~/.config/ox`),
recorded with `0600` permissions. The instance becomes the default for all
subsequent commands.

```bash
ox auth status    # resolves auth.me — instance + username, exit 3 if unauthenticated
ox auth switch    # pick a different default instance (picker, or pass the URL)
ox auth logout    # drop the stored token for the resolved instance
ox auth token     # print the resolved token (for scripts)
```

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

The token is resolved separately: `OXIDEAN_TOKEN` env, then the stored token
for the resolved instance. `ox config` exposes the local file:

```bash
ox config list                                   # config file + instances (tokens redacted)
ox config get default_instance
ox config set default_instance https://forge.example.com   # must be logged in
ox config set instances.https://forge.example.com.token oxidean_pat_…
```

`config get`/`list` never print stored tokens — they report `set`/`unset`.

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
ox pat list|create|revoke               # classic: --scopes repo,workflow,…
                                        # fine-grained: --fine-grained --contents read|write
                                        #   [--repo-access all|selected] [--repos id,id]
                                        #   [--perms issues=write,pull_requests=read,…]
ox invite accept --token <t>

# Instance admin (admin.* is session-only — not callable with PAT auth)
ox admin users list|ban|unban|delete|set-role|sessions|revoke-sessions|access|activity
ox admin invites list|create|link|revoke
ox admin runners | ox admin runner-token
ox admin lfs|git|mcp|auth [set …] | ox admin templates list|enable|disable|delete
ox admin auth set --provider-mode local|workos|oidc --allow-signup  # merges over current
ox admin repos gc

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
with `-R owner/name` anywhere a repo is expected. Content flags follow `gh`:
`--body` inline, `--body-file <path>` or `-` for stdin, and piped stdin fills
in automatically on verbs like `issue comment` and `secret set`.

Release assets are the one path off `/api/rpc`: `release upload` POSTs
multipart to `/api/repos/{o}/{n}/releases/{id}/assets` and `release download`
GETs `/api/releases/assets/{id}` — both accept `Authorization: Bearer <pat>`.

## Instance compatibility (capability manifest)

Before running an RPC-backed command — including `auth status` — `ox` fetches
`system.manifest` from the instance (public, like `system.health`) with a short
timeout. The manifest is the server's compatibility contract: which procedures
its dispatch table knows, its protocol and server versions, optional
capabilities (`mcp`, `rest`, `oauth`, `cli_dist`), and the oldest supported
CLI version.

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

Default output is human-readable text. `--json` prints the raw RPC envelope
verbatim — `{ "ok": true, "data": … }` on success, `{ "ok": false, "error":
{ "code", "message" } }` on failure — ready for `jq`:

```bash
ox repo list --json | jq '.data.repos[].name'
ox issue list octo/demo --state open --json \
  | jq -r '.data.issues[] | "\(.number)\t\(.title)"'
ox api repo.explore -F limit=5 --json | jq '.data'
```

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | success |
| 1 | API or transport error (any `{ok:false}` envelope, unreachable host) |
| 2 | usage error (bad flags, missing arguments, no instance configured, command unsupported by the instance manifest) |
| 3 | auth error (`auth.unauthenticated`, HTTP 401/403, missing token) |

## Environment

| Variable | Purpose |
| --- | --- |
| `OXIDEAN_INSTANCE` | Default instance URL when `--instance` is absent |
| `OXIDEAN_TOKEN` | Token override for any command (wins over the stored token) |
| `OXIDEAN_CONFIG_DIR` | Config directory override (default `~/.config/ox`) |

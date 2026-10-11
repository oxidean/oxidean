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

### For operators: serving binaries

The API serves every `ox-{rust-target-triple}` file found in
`OXIDEAN_CLI_DIST_DIR` (default `/usr/local/share/oxidean-cli`). Self-built
images stage the server's own platform; the published release image is built
with `OXIDEAN_CLI_DIST_URL` pointing at the GitHub release and stages the
full matrix (fetched + sha256-verified at image-build time). To offer more
targets yourself, build with that arg or drop additional binaries into the
dist dir:

```text
/usr/local/share/oxidean-cli/
  ox-x86_64-unknown-linux-gnu
  ox-aarch64-apple-darwin
  ox-x86_64-pc-windows-msvc
```

`GET /cli/latest` reports the version (the API package version) and the
sha256/size of every target found; `/cli/bin/{file}` serves artifacts and
digests unauthenticated so installs work before `ox auth login` exists.

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
```

Login writes `config.json` under the config directory
(`$OXIDEAN_CONFIG_DIR`, else `$XDG_CONFIG_HOME/ox`, else `~/.config/ox`),
recorded with `0600` permissions. The instance becomes the default for all
subsequent commands.

```bash
ox auth status    # resolves auth.me — instance + username, exit 3 if unauthenticated
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
for the resolved instance.

## Commands

```bash
# Repositories
ox repo list                       # your repos (repo.listMine)
ox repo list --owner someorg       # an owner's repos (repo.listByOwner)
ox repo view octo/demo

# Issues
ox issue list octo/demo [--state open|closed|all] [--author u] [--assignee u]
                        [--label l] [--search q] [--limit n]
ox issue view octo/demo 12
ox issue create octo/demo --title "bug report" --body "steps…"

# Pull requests
ox pr list octo/demo [--state open|closed|merged|all] [same filters]
ox pr view octo/demo 7
ox pr checks octo/demo 7           # commit statuses on the PR head

# Actions
ox run list octo/demo [--limit n] [--page n]
ox run view octo/demo <run-id>

# Packages
ox pkg list [--owner octo]

# Escape hatch — any procedure, raw input fields
ox api repo.get -f owner=octo -f name=demo
ox api issue.get -f owner=octo -f name=demo -F number=12
```

`-f` sends the value as a string; `-F` parses the value as JSON (use it for
numbers, booleans, arrays, objects). `owner/name` positionals may be replaced
with `-R owner/name` anywhere a repo is expected.

## Instance compatibility (capability manifest)

Before running an RPC-backed command — including `auth status` — `ox` fetches
`system.manifest` from the instance (public, like `system.health`) with a short
timeout. The manifest is the server's compatibility contract: which procedures
its dispatch table knows, its protocol and server versions, optional
capabilities (`mcp`, `rest`, `oauth`), and the oldest supported CLI version.

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

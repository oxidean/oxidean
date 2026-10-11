---
name: ox
description: >
  Use the `ox` CLI to work with an Oxidean forge instance — repositories, issues,
  pull requests, actions runs, releases, packages, notifications, and admin tasks —
  from the shell. Use this skill when: (1) you need to create, review, merge, or
  inspect issues/PRs/repos on an Oxidean instance, (2) CI scripts must talk to
  Oxidean without a browser, (3) you need --json/jq pipelines or the `ox api`
  raw-RPC escape hatch, (4) installing or authenticating `ox` itself.
license: MIT
compatibility: ox 0.1+; POSIX shells and PowerShell; no Node/Python required
metadata:
  homepage: https://github.com/oxidean/oxidean
  source: crates/oxidean-cli/skill/SKILL.md
---

# ox — the Oxidean CLI

`ox` is the single-binary command-line client for an Oxidean instance. Every
command maps to the instance's typed JSON-RPC surface; `ox api` reaches any
procedure that does not have a dedicated subcommand yet.

## Install

```sh
curl -fsSL <instance>/cli/install.sh | sh           # Linux / macOS / FreeBSD
irm <instance>/cli/install.ps1 | iex                # Windows PowerShell
ox self-update                                      # refresh to the instance's build
```

Installs to `~/.local/bin/ox` (`%LOCALAPPDATA%\Programs\ox` on Windows).
`OXIDEAN_INSTALL_DIR` overrides the destination; `OXIDEAN_BIN_BASE` overrides
the download mirror. Native packages exist for apt, dnf/zypper, apk, and
pacman (`.deb`, `.rpm`, `.apk`, `.pkg.tar.zst`).

## Authenticate

```sh
ox auth login --instance https://forge.example.com        # paste a PAT (TTY)
printenv OXIDEAN_TOKEN | ox auth login --instance <url> --token-stdin   # CI
ox auth status                                            # who am I
ox auth token                                             # print token (scripts)
ox auth logout                                            # drop stored token
```

Non-interactive environments: set `OXIDEAN_INSTANCE` + `OXIDEAN_TOKEN` and skip
`auth login` entirely. Config lives in `~/.config/ox/config.json`
(`OXIDEAN_CONFIG_DIR` overrides); multi-instance storage is keyed by URL.

## Command cheat sheet

| Area | Commands |
|---|---|
| repos | `repo list\|view\|create\|fork\|sync\|clone`, `repo edit\|rename\|archive\|unarchive\|delete\|transfer`, `repo star\|unstar\|watch\|unwatch`, `repo deploy-key`, `collaborator(s)`, `invites`, `branch-protection`, `tag-protection`, `mirror`, `merge-settings`, `lfs`, `quota` |
| issues | `issue list\|view\|create\|edit\|close\|reopen\|delete\|comment` |
| prs | `pr list\|view\|create\|merge\|diff\|files\|review\|comment\|ready\|update-branch\|checks\|close\|reopen\|checkout` |
| actions | `run list\|view [--log]\|rerun\|cancel`, `workflow list\|view\|run`, `secret list\|set\|delete`, `status create` |
| releases | `release list\|view\|create\|edit\|delete`, `release upload\|download\|delete-asset` |
| everything else | `search`, `browse`, `pkg list`, `org …`, `user …`, `notification …`, `label …`, `webhook …`, `ssh-key`, `gpg-key`, `email`, `pat …`, `invite accept`, `admin …` |

`<o/n>` is the `owner/name` repo spec; `-R/--repo` supplies it as a flag.
`ox <group> <sub> --help` prints per-command usage.

## Scripting: `--json` and exit codes

Every command accepts `--json` for the raw `{ok, data|error}` envelope —
lossless, stable, and the right input for `jq`:

```sh
ox issue list o/n --json | jq -r '.data.issues[].title'
ox api repo.get -f owner=o -f name=n --json | jq '.data.default_branch'
```

Exit codes: `0` ok · `1` API/transport error · `2` usage error or unsupported
command · `3` authentication error. Script on these, not on stderr text.

## Capability gating

`ox` fetches the instance's `system.manifest` before RPC commands and fails
fast (exit 2) when a procedure is missing — older or unreachable-manifest
servers fail open. If a command is gated out, `ox api <procedure>` tells you
whether the instance can serve it at all.

## The escape hatch: `ox api`

```sh
ox api <procedure> [-f field=value]... [-F field=<json>]...
ox api issue.comments.create -f owner=o -f name=n -F number=4 -f body='looks good'
```

`-f` values are strings; `-F` values are parsed as JSON (numbers, booleans,
arrays, objects). Input merges top-level into the RPC `input` object.

## Agent recipes

```sh
# Triage: list open issues, label one, comment
ox issue list o/n --json | jq -r '.data.issues[] | "\(.number)\t\(.title)"'
ox issue edit o/n 12 --label bug
ox issue comment o/n 12 --body-file - <<'EOF'
Reproduced on main — logs attached.
EOF

# Review and merge a PR, waiting for checks
ox pr checks o/n 34 --json | jq '.data.statuses[] | select(.state != "success")'
ox pr review o/n 34 --approve
ox pr merge o/n 34 --squash

# Create a repo and push initial code
ox repo create my-lib --visibility private
ox repo clone o/my-lib

# Run a workflow and watch it
ox workflow run o/n ci.yml --ref main
ox run view o/n --id <run-id> --json | jq '.data.run.status'

# Publish a release with artifacts
ox release create o/n v1.2.0 --title "v1.2.0" --notes-file CHANGELOG.md
ox release upload o/n v1.2.0 dist/*.tar.gz
```

## Install this skill

```sh
ox skill print                                   # raw SKILL.md to stdout
ox skill install --harness claude,cursor --project   # repo-local skill dirs
ox skill install --harness devin --global        # user-level
curl -fsSL <instance>/cli/skill.md               # fetched copy, versioned with the server
```

On a TTY, `ox skill install` prompts for harness(es) and project/global scope.

## Notes for agents

- Always pass `--json` when parsing output; the human-readable renderers are
  for people and may change shape.
- Prefer `ox api` over guessing flags on brand-new commands — it reaches every
  procedure the instance exposes.
- `release upload`/`download` use multipart HTTP routes, not RPC; they still
  honor the same instance/token resolution.
- Non-TTY commands never prompt; missing input fails fast with exit 2.

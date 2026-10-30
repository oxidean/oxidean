# U5 Commit Lint Gates — Functional Spec

## Purpose

The enforcement surfaces for U2's grammar: the opt-in local `.githooks/commit-msg` hook (FR1.1) and the authoritative `ci-commit` CI gate (FR1.2) covering the PR commit range, the PR title (FR8.1), and registry completeness (FR2.2 CI side).

## Workflows

### WF-1 Hook install (`make setup`)

1. `git config core.hooksPath .githooks` — warns with the old value if a foreign hooksPath existed (BR5.2).
2. `.githooks/commit-msg` is executable; it invokes `bun scripts/versioning/cli.ts commit-check --file "$1"` (shared parser — BR2.5).
3. Bypass: `git commit --no-verify`; uninstall: `git config --unset core.hooksPath`.

### WF-2 `commit-msg` hook run

1. Read the message file → `commit-check --file`.
2. Exit 1 → print failure lines, commit aborts (BR5.1). Exit 0 (incl. warnings) → commit proceeds.

### WF-3 `ci-commit` job (`.github/workflows/`)

Trigger: `pull_request` (opened/synchronize/reopened) + `push` to main.

1. Checkout with `fetch-depth: 0` (full range available).
2. `commit-check --range ${{ github.event.pull_request.base.sha }}..${{ github.event.pull_request.head.sha }}` (push events: `before..after`) — BR5.3.
3. `commit-check --pr-title "${{ github.event.pull_request.title }}"` (PR events only) — BR5.4.
4. `versioning check` — registry completeness vs workspace enumeration (BR5.5).
5. Any check exit 1 → job fails; warnings print; job uses `contents: read` only (BR5.6).

## States

The gate is a pure function of the pushed refs: `pass`, `warn` (green with stderr warnings), `fail`. No state persists between runs.

## Interfaces

- Consumes: U2 `commit-check` (all three modes), U1 `check` subcommand.
- Consumed by: U4's release workflows live in separate jobs/files; U7 documents install + bypass.
- Local dev loop: `make setup` (install), `make commit-check` convenience target wrapping the tooling.

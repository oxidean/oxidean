# Build Instructions — semver-release-automation

## Dependencies

- Bun `1.4.0` (root `package.json` `packageManager`) — `corepack enable && bun install`
- Rust stable + Cargo — `cargo metadata -q` warms the workspace
- `cargo-nextest` (optional but recommended; `cargo test` fallback)
- Docker + Compose v2 (`docker compose version`)
- `act` (workflow dry-runs; `act --version`)
- `gh` (release workflows only — not needed for local build/test)

## Environment setup

- `cp .env.example .env` — optional; no runtime services are needed for the
  versioning toolchain or its tests.
- `make setup` — installs `.githooks/commit-msg` via
  `git config core.hooksPath .githooks` (opt-in, explicit).
- No new env vars: U6 deleted `OXIDEAN_RPC_VERSION`; `OXIDEAN_CLI_DIST_DIR`
  defaults to `/usr/local/share/oxidean-cli` and only matters for the
  `/cli/latest` dist server.
- `OXIDEAN_PUBLIC_ORIGIN`, `SOURCE_COMMIT` — release/distribution
  provenance only; unset is fine locally.

## Build commands

```bash
bun install --frozen-lockfile     # JS deps (turbo workspaces)
cargo check --workspace           # Rust type/build check
cargo build --workspace           # full Rust build (needed for `ox` binary paths)
make rpc-gen                      # regenerate packages/api-client from rpc_gen.rs
make openapi-gen                  # regenerate docs/openapi.yaml
bun scripts/versioning/cli.ts check   # versioning registry validation
```

## Build verification

```bash
make version-check                # registry + literal-version completeness: "10 components registered"
cargo check --workspace           # exit 0, no warnings-as-errors failures
bun scripts/versioning/cli.ts --help 2>/dev/null || bun scripts/versioning/cli.ts  # command surface lists all 7 subcommands
```

## Troubleshooting

- **`bun.lock` drift after `versioning bump`** — bump runs
  `bun install --lockfile-only`; re-run `bun install` if the lockfile
  diverges.
- **Cargo.lock drift** — `cargo check` regenerates it; commit it.
- **Hook not firing** — `git config --get core.hooksPath` must print
  `.githooks`; re-run `make setup`.
- **`commit-check` rejects a merge** — merge/revert/`chore(release):`
  subjects are exempt by design; anything else needs `type:`.
- **`act` can't find workflows** — run from repo root; `act -l -W
  .github/workflows/<file>.yml` validates parse.
- **Docker daemon absent** — `docker compose config` still works for
  syntax validation; `up`/smoke lanes need a live daemon.

## Sources

Consumes per unit `code-generation-plan.md`, `unit-test-instructions.md`,
and `code-summary.md` under `construction/*/code-generation/` (all seven
units), plus the stage-level Testing Contract they embed.


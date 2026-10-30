# Build Instructions — oxpilot

Workspace: Bun + Turborepo (`apps/*`, `packages/*`) + Cargo workspace (`crates/*`).

## Dependencies

```bash
bun install
cargo fetch        # implicit on first build; rust-toolchain pins the toolchain
```

## Environment

- `.env` from `.env.example`; oxpilot keys are `OXIDEAN_OXPILOT_*` (provider, model, budgets, `OXIDEAN_OXPILOT_ENABLED` feature flag following the `OXIDEAN_MCP_ENABLED` pattern).
- DB dialect via `OXIDEAN_DATABASE_URL` (Postgres / MySQL / SQLite); migrations `0087`–`0090` ship the oxpilot schema.
- Local dev stack: `make up-with-dev-auth` (Compose + Mailpit/OIDC/stubs).

## Build commands

```bash
cargo build --workspace --all-targets          # Rust crates
bun run --filter @oxidean/web build            # Astro + Octane web bundle
make rpc-gen                                   # regenerate packages/api-client (commit the diff)
make rpc-sync-check                            # client drift gate — must be clean
```

## Verification

```bash
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
make web-lint && make web-format-check
make dead-code-check                           # knip + cargo machete + clippy dead_code
make route-coverage-check && make browser-coverage-check
git diff --check
```

CI is authoritative (`ci.yml`: rust-build, web-octane, web-browser, browser-coverage, dead-code, route-coverage, db-dialects, e2e-stack, coverage-weighted, commit-lint, rpc-sync; `images.yml`; `ox-cli.yml`). Local heavyweight Rust/Bun builds are deferred to CI per user directive — the recorded construction verification command is:

```bash
echo "local unit verification deferred to CI per user directive" && git diff --check
```

## Troubleshooting

- `rpc-sync-check` drift → run `make rpc-gen` and commit the generated diff; never hand-edit `packages/api-client`.
- `web-octane` format failures → `bun run --filter @oxidean/web format`.
- Clippy `-D warnings` → fix, never `#[allow]` new denials.
- `.tsrx` authoring rules — see `.agents/skills/octane/SKILL.md` before touching UI.

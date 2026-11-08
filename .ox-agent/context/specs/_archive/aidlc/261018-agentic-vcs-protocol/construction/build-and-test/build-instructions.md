# Build Instructions — agentic-vcs-protocol

Scope: the `oxidean-vcs` crate (server + client features), its host wiring in
`oxidean-api`, thin glue in `oxidean-cli`, MCP descriptors, the `ox vcs`
16-command surface, `apps/web` settings/activity surfaces, and the regenerated
`@oxidean/api-client`.

## Prerequisites

- Rust toolchain per `rust-toolchain.toml` (stable + `cargo-nextest` per
  `docs/TESTING.md`).
- Bun for the web workspace (`apps/web`, `packages/*`).
- Docker + Compose — preferred for debugging/e2e (`make up-with-dev-auth`).
- Postgres/MySQL optional: dialect lanes run only where `DATABASE_URL` is set
  (SQLite coverage always runs — NFR6 recorded debt).

## Dependency installation

```bash
bun install                 # web + packages workspace
cargo fetch                 # Rust workspace deps (ci images pre-cook via chef)
```

## Environment setup

New operator surface (documented in `.env.example` + `docs/CONFIGURATION.md`):

| Var | Default | Purpose |
|-----|---------|---------|
| `OXIDEAN_VCS_OBJECTS_DIR` | `./var/vcs-objects` | object CAS store root |
| `OXIDEAN_VCS_MAX_OBJECT_BYTES` | `268435456` (256 MiB) | per-object cap, enforced mid-stream |
| `OXIDEAN_VCS_REPO_QUOTA_BYTES` | unset (unbounded) | per-repo object quota |
| `OXIDEAN_VCS_GC_INTERVAL_SECS` | `3600` | GC sweep cadence |
| `OXIDEAN_VCS_GC_GRACE_SECS` | `86400` | unlinked-object grace window |

## Build commands

```bash
cargo build -p oxidean-vcs                     # default (client) feature set
cargo build -p oxidean-vcs --all-features      # server feature set
cargo build --workspace                        # full workspace
bun run --filter @oxidean/web build            # web app
make rpc-gen                                   # regenerate @oxidean/api-client (never hand-edit)
```

## Build verification

```bash
cargo check --workspace
cargo clippy -p oxidean-vcs -p oxidean-cli -p oxidean-api --all-features
make web-lint                  # type-aware oxlint + octane DOM-race heuristic
make web-format-check
```

## Feature boundary check (client must not pull server deps)

```bash
cargo tree -p oxidean-vcs --no-default-features --features client \
  | grep -E "oxidean-db|oxidean-git|sqlx" || echo "boundary clean"
```

## Troubleshooting

- `rpc-sync-check` drift after changing `vcs.*` DTOs: re-run `make rpc-gen` and
  commit the regenerated client — never edit `packages/api-client` output.
- Migrations: VCS tables ship three-dialect filename parity
  (`crates/oxidean-db/migrations`); a missing dialect file fails CI.
- Object-store writes fail on a fresh clone: `OXIDEAN_VCS_OBJECTS_DIR` is
  created lazily by `ObjectStoreConfig::from_env`; ensure the parent is
  writable.
- `vcs.disabled` on writes: repo must be `vcs_enabled` via `vcs.repo_enable`
  (admin-class); archived repos deny writes, never-enabled deny too.

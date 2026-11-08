# Plan 01-01 Summary — Monorepo skeleton

**Completed:** 2026-09-09
**Status:** done

## What shipped
- Bun + Turborepo JS workspace (`apps/*`, `packages/*`), `packageManager: bun@1.4.0`
- Cargo workspace: `oxidean-api`, `oxidean-core`, `oxidean-db` stubs
- `@oxidean/api-client` stub (`RPC_VERSION = 1`) and `@oxidean/web` workspace dep
- Makefile (`help`, `dev`, `rpc-gen`, `rpc-sync-check`, `up`, `down`, `test`, `smoke`)
- `.env.example`, `.gitignore`, `scripts/check-rpc-sync.sh`
- README development section (Bun preferred, pnpm fallback)

## Verification
- `bun install` ✓
- `cargo metadata --no-deps -q` ✓
- `make help` / `make rpc-sync-check` ✓

## Decisions
- Pinned `bun@1.4.0` to match local toolchain (plan suggested 1.2.5)

## Next
Plan 01-02 — Axum RPC + specta codegen

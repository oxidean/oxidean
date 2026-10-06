# Plan 01-02 Summary — Axum RPC + codegen

**Completed:** 2026-09-09
**Status:** done

## What shipped
- `oxidean-core` RPC types (`AppError`, health/echo, protocol version)
- `oxidean-db` Postgres pool via sqlx + `ping()` (`skipped` without `DATABASE_URL`)
- Axum API: `/healthz`, `POST /api/rpc`, `GET /api/rpc/ws`
- `Oxidean-RPC-Version: 1` gate; CORS (dev mirror / prod allowlist required)
- Procedures `system.health`, `system.echo` (8KiB cap)
- `rpc-gen` binary emits `packages/api-client` + TanStack Query helpers / D-21 aliases
- Tests: HTTP (4) + WS (1) + cors unit; vitest client header check
- `make rpc-gen` / `make rpc-sync-check` (`git diff --exit-code`)

## Health + DB
Health stays HTTP 200 when DB is down/unconfigured; `database` field is `ok` | `error` | `skipped`.

## Deviation
- Used embedded TS emit from Rust SoT instead of specta-typescript crate (same outcome: Rust owns shapes, CI drift check). Can swap to specta later without protocol change.

## Next
Plan 01-03 — Octane Start web + UI-SPEC

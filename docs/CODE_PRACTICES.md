# Code practices

Conventions for Oxidean humans and agents. Complements [CONTRIBUTING.md](../CONTRIBUTING.md) and [AGENTS.md](../AGENTS.md).

## Monorepo boundaries

| Path | Owns | Must not |
|------|------|----------|
| `apps/web` | Octane UI, routes, Vite, Vitest projects | Dialect SQL; hand-written RPC DTOs as source of truth |
| `packages/api-client` | Generated TS client | Manual “fixes” without regenerating from Rust |
| `crates/oxidean-api` | HTTP/RPC, auth, email, handlers | DB dialect `if` trees |
| `crates/oxidean-core` | Shared domain types / pure logic | I/O, SQL, Axum |
| `crates/oxidean-db` | Migrations + `Database` API (all dialects) | Product UI concerns |

Prefer the smallest change that fits an existing pattern. New dependencies need a clear gap (prefer `@octanejs/*` on the web; prefer crates already in the Cargo workspace on the API).

## Versioning & license

Workspace version is **0.1.0** (Cargo `[workspace.package]` and package `package.json` files). License is **MIT** ([LICENSE](../LICENSE)).

## Rust

- Edition 2021; workspace `license` / `version` / `edition`.
- Prefer `Result` + `thiserror` in libraries; avoid `unwrap`/`expect` outside tests.
- Keep auth and privilege checks explicit (`require_verified`, admin gates) — do not weaken for convenience.
- Integration tests under `crates/*/tests/` for cross-module behavior; unit tests colocated when pure.
- Format with `rustfmt`; CI runs `cargo nextest` (see `.config/nextest.toml`).

## TypeScript / Bun

- Bun is the package manager; respect `packageManager` and lockfile (`bun.lock`).
- Prefer existing path aliases (`@/…` in web) over deep relative imports.
- Colocate tests: `*.unit.test.ts`, `*.integration.test.ts`, e2e under `apps/web/e2e/`.
- Do not edit generated `packages/api-client` by hand — change Rust, then `make rpc-gen`.
- **Lint / format / types (web):** `@tsrx/oxc` — `make web-lint` (type-aware `oxlint --deny-warnings`) and `make web-format-check` (`oxfmt`). CI runs both on `web-octane`. Run them before committing web changes. No ESLint/Prettier.

## Octane UI

Full skill: [`.agents/skills/octane/SKILL.md`](../.agents/skills/octane/SKILL.md).

- Author in **`.tsrx`** with Rivet templates (`@{`, `@if`/`@else`, `@for`).
- Do not mix React `return (` JSX with Rivet directives in one component.
- Server/session data: TanStack Query (`apps/web/src/lib/session-queries.ts`). Forms: `@octanejs/tanstack-form`. File uploads: `@octanejs/dropzone` / `FileDropzone`.
- Text fields: native `onInput` (or `field.handleChange`). Anonymous auth pages: SSR loaders, no decorative form skeletons.
- Preserve chrome / brand patterns; do not introduce a second design system.
- **DOM races:** do not `@if`/`@else`-swap large sibling trees next to Base UI `RadioGroup` / Select, and do not nest `@if`/`@for` inside `form.Subscribe` bodies that re-render on checkbox/radio clicks (causes `insertBefore` / “Something went wrong!”). Keep both panels mounted and toggle with `hidden`, use per-field `form.Field`, or isolate the swap in a child component. Wrap multi-root `@if` bodies in `<>…</>`. Prefer `keepMounted` on Checkbox/Radio Indicators (shipped in `components/ui`). Happy-dom integration, Vitest browser (`*.browser.test.tsx`), and stack-browser e2e fail automatically on these races (`setup-integration.ts`, `setup-browser.ts`, `newGuardedPage` / `assertNoOctaneOverlay`).
- **Browser coverage required for high-risk UI:** any new or changed `.tsrx` that uses `Checkbox`, `RadioGroup` / `RadioGroupItem`, `form.Subscribe`, Select, Switch, or Dialog/Menu portals must register Chromium proof in `apps/web/src/test/browser-coverage.manifest.ts`. Prefer a colocated `*.browser.test.tsx` (use `pickSelectOptionByTestId` for Base UI Select). Run `make browser-coverage-check-pr` before push. Bootstrap `skip` is inventory-only — **CI fails if you add or edit a skip-only surface without real evidence**. See [TESTING.md](TESTING.md).

## RPC & API

- Procedure names and shared types in Rust are authoritative.
- Client regeneration is part of the change: `make rpc-gen` + commit.
- Stable error codes matter for UI (`auth.email_unverified`, etc.) — don’t rename casually.
- Cookies / CORS / Secure flags follow `OXIDEAN_ENV` — see [CONFIGURATION.md](CONFIGURATION.md).

## Testing expectations

| Change type | Minimum |
|-------------|---------|
| Pure helper | Unit test |
| UI + Query / session | Integration test (`renderWithQueryClient` where applicable) |
| High-risk interactive UI (Checkbox / Radio / Select / Switch / portals / `form.Subscribe`) | Vitest browser (`*.browser.test.tsx`) + `browser-coverage` manifest row (`make browser-coverage-check-pr`) |
| Auth / RPC contract | Rust integration test and/or stack e2e |
| RPC schema | `make rpc-sync-check` clean |

Details: [TESTING.md](TESTING.md).

## Docs & comments

- Update the nearest package README when a package’s purpose or public commands change.
- Prefer linking to `docs/*` over duplicating long guides.
- Comments explain **why** (threat model, dialect quirk, Octane pitfall), not what the next line does.
- Do not commit planning chatter into product docs unless asked; GSD lives under `.planning/`.

## Security & privacy

- No secrets in git, CI logs, or README examples.
- Privileged actions stay behind verification / admin checks already established in auth phases.
- Factory reset and similar destructive RPCs require explicit confirmation strings — never weaken.
- Uploads and session cookies: follow existing size/type and cookie attribute patterns.

# Code style

Distilled conventions; canonical detail lives in
[`docs/CODE_PRACTICES.md`](../../docs/CODE_PRACTICES.md) and
[`AGENTS.md`](../../AGENTS.md).

## Shared

- Prefer the smallest change fitting an existing pattern; no new frameworks,
  state libraries, or UI kits without a clear gap.
- Dialect SQL lives only in `crates/oxidean-db` (migrations authored ×3 in
  `migrations/{postgres,mysql,sqlite}`); nothing else branches on dialect.
- No secrets in repo, CI, or docs examples — use `.env.example` /
  `docs/dev-auth.env.example`.
- No AI/tool attribution on commits or PRs
  (`.cursor/rules/no-ai-attribution.mdc`).
- Generated sources are read-only (`packages/api-client`,
  `docs/openapi.yaml`): change the Rust producers, regenerate
  (`make rpc-gen`, `make openapi-gen`), commit output in the same change.
- Comments explain *why* (threat model, dialect quirk, Octane pitfall) and
  cite `docs/ARCHITECTURE.md` decision IDs.
- New `OXIDEAN_*` env vars land in `.env.example` + `docs/CONFIGURATION.md`
  in the same PR.
- New Makefile targets are `.PHONY`-declared with a runnable recipe
  (`scripts/check-makefile.sh` enforces).
- New crates register a `versioning.toml` component/scope.

## Rust

Edition 2021, `cargo fmt` defaults, `clippy -D warnings` + `-D dead_code` +
`cargo machete` gated in CI. Error idioms by layer: `thiserror` enums in
libraries; `oxidean-db` façade returns `Result<T, String>` (error text is a
contract — never reword casually); API handlers return
`Result<T, AppError>` with stable `domain.reason` snake-dotted codes. No
anyhow/eyre; binaries fail fast. Internals never leak: `tracing::error!`
the detail, return a generic message. `mod.rs` façades, `*Row` / `*Request`
/ `*Response` / `*Service` / `*Store` / `*Limiter` suffix taxonomy;
db verbs `insert_/find_*_by_/list_/update_/count_/delete_/consume_/revoke_`.

## TypeScript / web

Octane `.tsrx` islands — never React JSX (see `web-ui.md`). Type gate is
`make web-lint` (type-aware oxlint); format is `oxfmt` + Prettier for
`.astro` (`make web-format-check`). No ESLint; `knip` gates dead exports.
Kebab-case files, PascalCase components, `@/` alias, `import type`.
TanStack Query owns server/session data; `useState` owns local UI only.

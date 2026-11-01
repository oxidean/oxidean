# Agent guide — Oxidean

Short orientation for coding agents and automated contributors. Humans: start with [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Product

Self-hostable code forge (git, issues, pull requests, orgs, LFS, releases, packages, actions). **One codebase** for Oxidean Cloud and self-hosted. Bun + Turborepo (`apps/*`, `packages/*`) and a Rust Cargo workspace (`crates/*`).

## Stack (do not invent alternatives)

| Layer | Tech |
|-------|------|
| Web UI | **Octane** (`.tsrx`) islands on **Astro** static shells (`apps/web/src/pages/**`); TanStack Query / Form via `@octanejs/*`; file pickers via `@octanejs/dropzone` |
| Web serving | **`oxidean-web`** Rust binary (`crates/oxidean-web`) — serves Astro `dist`, injects theme/title/session metas per request, gates protected routes, reverse-proxies API prefixes via `OXIDEAN_API_ORIGIN` |
| API | Rust Axum + typed JSON RPC (`oxidean-api`) |
| Domain types | `oxidean-core` |
| Persistence | `oxidean-db` (Postgres / MySQL / SQLite) |
| TS RPC client | `@oxidean/api-client` — **generated** by `make rpc-gen` |

Canonical docs: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/API.md](docs/API.md), [docs/CONFIGURATION.md](docs/CONFIGURATION.md).

## Octane is not React JSX

The web app uses **Octane**, not React as the UI runtime. React-shaped hooks exist; template authoring does **not**.

Before editing UI under `apps/web`:

1. Read the project skill [`.agents/skills/octane/SKILL.md`](.agents/skills/octane/SKILL.md).
2. Prefer official reference: https://octanejs.dev/llms.txt and https://octanejs.dev/docs/differences-from-react
3. Author components in **`.tsrx`** with `function Comp() @{ … }`, `@if` / `@else` (no `@else if`), `@for`, native `onInput` for text fields.
4. Never mix `return (` JSX with Rivet `@{` / `@if` in the same component — that breaks exports/hydration.
5. Server domain data: **TanStack Query** (`@octanejs/tanstack-query`) via `apps/web/src/lib/session-queries.ts` and friends. Forms: **`@octanejs/tanstack-form`** (`useForm` / `form.Field`, text via `onInput` + `field.handleChange`). File uploads: **`@octanejs/dropzone`** via `apps/web/src/components/ui/file-dropzone.tsrx` (does not upload — callers own `FormData`/`fetch`). Local ephemeral UI state may still use `useState`. Do not add Zustand for server/session data.

## Hard boundaries

- **Do not hand-edit** generated client sources as source of truth — change Rust RPC / types, then `make rpc-gen`.
- Dialect SQL lives in `crates/oxidean-db` only; API must not branch on DB dialect.
- No production secrets in repo, CI, or docs examples. Use `.env.example` / `docs/dev-auth.env.example`.
- Prefer extending existing patterns over new frameworks, state libraries, or UI kits.
- **No AI attribution** on commits or PRs: never add `Co-authored-by` / `Generated with` / `Made-with` (or similar) for Cursor, Claude, Copilot, or any AI/tool unless the user explicitly asks in the current turn. See [`.cursor/rules/no-ai-attribution.mdc`](.cursor/rules/no-ai-attribution.mdc).

## In-repo skills (always)

When a task matches a skill under [`.agents/skills/`](.agents/skills/), **read and follow that skill** before inventing a workflow. Cursor rule: [`.cursor/rules/use-in-repo-skills.mdc`](.cursor/rules/use-in-repo-skills.mdc). Claude Code loads the same trees via the [`.claude/skills/`](.claude/skills) symlink. Examples: Octane UI → `octane`; Railway/deploy → `use-railway`; Rust async → `rust-async-patterns`; prose finalize → `clean-user-facing-text`; watermark strip → `remove-ai-marks`; commit subjects/version bumps → `semver-conventions`.

## AI watermarks / provenance hygiene

Vendored skills (pin and refresh notes in [`.agents/skills/README-watermarks.md`](.agents/skills/README-watermarks.md)):

- [`.agents/skills/remove-ai-marks/`](.agents/skills/remove-ai-marks/) — strip Unicode / C2PA / container AI metadata via the watermarks-remover HTTP service
- [`.agents/skills/clean-user-facing-text/`](.agents/skills/clean-user-facing-text/) — self-contained prose hygiene (no service)

Claude Code also loads the same skills from [`.claude/skills/`](.claude/skills/) (symlinks to `.agents/skills/`).

`remove-ai-marks` auto-starts the HTTP service for the run via
[`.agents/skills/remove-ai-marks/scripts/oxidean-watermarks-service.sh`](.agents/skills/remove-ai-marks/scripts/oxidean-watermarks-service.sh)
(`ensure` → work → `teardown`). Checkout: gitignored `tmp/watermarks-remover`
(cloned on demand). Do not vendor `service/` into this repo. Do not invent
local cleaners if ensure fails. Use `clean-user-facing-text` for offline
prose-only passes (no service).
## Commands agents should know

```bash
make help
make rpc-gen                 # regenerate @oxidean/api-client
make test                    # Rust nextest + Vitest
make test-e2e-stack          # full auth stack e2e
make up-with-dev-auth        # preferred local stack: Compose + Mailpit/OIDC/stubs attached
make up / make smoke         # Compose + health
make rpc-sync-check          # CI gate for client drift
make web-lint                # oxlint type-aware (apps/web; @tsrx/oxc) + octane DOM-race heuristic
make web-format-check        # oxfmt + prettier --check (apps/web)
make dead-code-check         # knip (.ts/.tsrx/.astro) + cargo machete + clippy -D dead_code
make test-web-browser           # Vitest Chromium component DOM-race tests
make browser-coverage-check-pr  # high-risk UI Chromium gate (change-aware vs origin/main)
```

### Before committing web changes

Always run (or equivalent filter scripts) and fix failures before claiming done or creating a commit:

```bash
make web-lint
make web-format-check
```

`make web-lint` is **type-aware** (`oxlint --type-aware` via `oxlint-tsgolint`) and also runs `scripts/check-octane-dom-races.ts` (RadioGroup + sibling `@if`/`@else` heuristic). Treat those diagnostics as the web type gate — plain `tsc --noEmit` does not understand `.tsrx` yet (needs `@tsrx/typescript-plugin`; peer range is still TS 5.9.x while this app uses TypeScript 7). Also fix editor/linter type diagnostics you introduce. Format with `bun run --filter @oxidean/web format` when `format:check` fails.

See [docs/TESTING.md](docs/TESTING.md) and [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## CI images

PR CI lanes run inside self-owned images (`ghcr.io/<owner>/oxidean-ci:{dev,release,cov,tools}`) built from [`docker/ci-base/Dockerfile`](docker/ci-base/Dockerfile) by `.github/workflows/ci-base.yml`; `ci.yml`'s `ci-image` job pins each to `@sha256` per run via `scripts/ci-image-tag.sh`. There is no cross-run Rust cache — dep artifacts are cargo-chef-cooked into the image, and workspace crates always compile from the fresh checkout. Do not re-add `Swatinem/rust-cache` or per-job toolchain installs to `ci.yml`. Details and refresh procedure: [docs/TESTING.md](docs/TESTING.md#ci-base-images-run-isolated-no-cross-run-rust-cache).

## Required status checks (merge gate)

The `Main` branch ruleset requires every `ci.yml` job plus `images.yml`'s `meta` to pass on a PR's head commit; path-filtered workflows (`ox-cli.yml`, `ci-base.yml`) and fork-empty matrices are deliberately excluded (a required check that never runs blocks the PR forever). **When you add, rename, or remove a job — including matrix values (`shard`, `dialect`, `svc`) — in a required workflow, update the ruleset's check list in the same change.** Procedure and the `gh api` calls: [docs/TESTING.md](docs/TESTING.md#required-status-checks-merge-gate).

## Planning / Agent OS

Structured work runs through **Agent OS** skills under `.agents/skills/` (`agent-os-plan-product`, `agent-os-shape-spec`, `agent-os-discover-standards`, `agent-os-index-standards`, `agent-os-inject-standards`) plus `bm-prd-creator` for idea→PRD→milestones. Product docs live under `agent-os/product/`, team standards under `agent-os/standards/`, and specs under `agent-os/specs/` — one canonical home each; `.claude/skills`, `.devin/skills`, and root `CLAUDE.md` are symlinks, never second copies. The shipped v1.0 GSD-era milestone history and the retired AI-DLC intent records are archived read-only at [`agent-os/specs/_archive/`](agent-os/specs/_archive/). Post-Phase-06 polish (Query session cache, setup auth stack, factory reset) is recorded in `agent-os/specs/_archive/gsd/milestones/v1.0-phases/06-self-host-admin-bootstrap/deferred-items.md`.

Chat-driven agents (Telegram/messaging surfaces, ACP hosts — no plan mode or interactive question forms) follow [`.agents/skills/agent-os-async-flow/SKILL.md`](.agents/skills/agent-os-async-flow/SKILL.md): it adapts the upstream skills' plan-mode/AskUserQuestion requirements for async conversations — autonomous spec drafts, open questions in plain text, review via draft PR.

## Scratch files (`tmp/`)

Use repo-root [`tmp/`](tmp/README.md) for agent and local scratch (screenshots, dumps, one-off scripts, debug logs). The directory is gitignored except `tmp/README.md`. **Never** drop temporary files in the repository root or inside `apps/` / `crates/` / `packages/` source trees.

## Contribution standards

Follow [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/CODE_PRACTICES.md](docs/CODE_PRACTICES.md). Cursor rules under `.cursor/rules/` encode the same expectations for agents.

**Commits are SemVer-prefixed and enforced**: every commit subject must match `type(optional-scope): subject` (`feat`, `fix`, `docs`, `chore`, …). A `commit-msg` hook checks locally (`make setup` installs it — `--no-verify` bypasses, but CI `commit-lint` still checks the PR range + title). Subjects drive per-component SemVer bumps and changelogs — pick accurate types/scopes. Full grammar, bump semantics, and release flow: [docs/VERSIONING.md](docs/VERSIONING.md); agent checklist: [`.agents/skills/semver-conventions/SKILL.md`](.agents/skills/semver-conventions/SKILL.md).


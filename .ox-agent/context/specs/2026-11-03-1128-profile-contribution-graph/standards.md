# Standards — profile contribution graph

Standards binding on this work, inlined from `.ox-agent/context/standards/`.

---

## testing

Posture: **test-after** — implement the change, then write and run the tests
its change-type minimum requires; full CI must be green before merge.

Minimums by change type that apply here:

| Change type | Minimum | This spec |
|-------------|---------|-----------|
| UI + Query / session | happy-dom integration test | extend `$owner.user-profile.integration.test.ts` |
| Auth / RPC contract | Rust integration test and/or stack e2e | `tests/user_contributions.rs` |
| RPC schema | `make rpc-sync-check` clean | new `user.getContributions` |
| Dialect-touching DB code | `dialect_*` test | `contributions.rs` / `authored_commits.rs` / `contribution_days.rs` ×3 dialects |

- Web gates before committing `apps/web`: `make web-lint` +
  `make web-format-check`.
- Route-/browser-coverage manifests are change-aware; zero `it.skip` in the
  tree.

## rust

- New handlers go in the domain crate — `oxidean-api-user` for
  `user.getContributions`, not the shell.
- Dialect SQL lives only in `crates/oxidean-db`; the API must not branch on
  dialect.
- `Database` façade delegates to modules; db verbs
  `list_/find_/insert_/count_/…`; `*Row` suffix for query rows.
- Error idioms: façade returns `Result<T, String>`; handlers return
  `Result<T, AppError>` with `domain.reason` codes (`user.not_found` here —
  same code as `get_public_profile` for unknown/banned users, preserving the
  anti-enumeration shape).
- Gates: `cargo fmt`, `clippy --workspace --all-targets -D warnings
  -D dead_code`, `cargo machete`.

## web-ui

- Octane `.tsrx`, Rivet templates (`@{`, `@if`/`@else`, `@for`) — never React
  JSX; load `.agents/skills/octane/SKILL.md` before editing.
- Server data via TanStack Query (`queryOptions` + `useQuery`); `useState`
  for local UI only.
- DOM races: don't `@if`/`@else`-swap sibling trees next to Base UI
  interactive components; prefer mounted panels + `hidden`.
- Pure-div rendering for the heatmap (insights page precedent) — no new chart
  dependency.

## code-style

- Smallest change fitting an existing pattern; no new frameworks.
- Generated sources are read-only: change Rust producers, `make rpc-gen`,
  commit output in the same change.
- Comments explain *why*; db verbs and `*Row`/`*Request`/`*Response` suffix
  taxonomy apply.
- Kebab-case files, PascalCase components, `@/` alias, `import type`.

## commits-versioning

- Subjects must match `type(scope): subject`; `feat(profile): …` /
  `feat(api): …` fit this work; spec commit is `docs(spec): …`.
- CI `commit-lint` checks the PR range and PR title.

## way-of-working

- PR-only into `main`, squash-merge, branch `feat/<slug>` for the
  implementation PR (this spec rides `spec/<slug>` per the async-flow skill).
- No AI/tool attribution on commits or PRs.

## review-practices

- Never hand-edit generated sources (`packages/api-client`); regenerate and
  commit output in the same change.
- Adding/renaming/removing a `ci.yml` job requires updating the `Main`
  ruleset's required-checks list in the same change (not expected here — no
  CI changes planned).

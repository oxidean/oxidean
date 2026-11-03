# User profile contribution graph and activity — plan

Roadmap item: **COL-12** (remainder; profile README already shipped).
One implementation PR: backend + web + tests together. Order below is the
suggested build sequence inside the branch.

## Task 1 — Types and RPC scaffolding

- `crates/oxidean-core/src/repo_types.rs`: add
  `GetContributionsRequest { username, from?: String, to?: String }` and the
  response family `UserContributions` / `UserContributionDay` /
  `UserContributionByType` / `UserContributionMonth` /
  `UserContributionSection` / `UserContributionRepo` /
  `UserContributionItem` (all `Serialize + Deserialize + JsonSchema`,
  matching the existing DTO style). `UserContributions` also carries
  `years: Vec<i64>` (account-creation year → current year, descending) for
  the GitHub-style year rail.
- `oxidean-api-user`: `contributions.rs` handler, register
  `"user.getContributions"` in `PROCEDURES` + `dispatch.rs`.
- `proc_classes.rs`: classify as `Open`; add to the public list test at the
  bottom of that file if one exists.
- `make rpc-gen` → `@oxidean/api-client` gains `user.getContributions`.

Verify: `cargo check -p oxidean-api-user -p oxidean-core` compiles;
`make rpc-sync-check` clean.

## Task 2 — Migration 0094 (all three dialects)

- `migrations/{postgres,mysql,sqlite}/0094_user_contribution_indexes.sql`:
  - `idx_repository_activity_actor_created ON repository_activity(actor_id, created_at DESC)`
  - `idx_pull_reviews_author ON pull_reviews(author_id, submitted_at DESC)`
- Match each dialect's index idiom (MySQL lacks `IF NOT EXISTS` on indexes in
  older files — copy the exact style of the neighboring migration in each
  dir).

Verify: `make test` dialect boot (sqlite migration applies); CI dialect
lanes.

## Task 3 — `oxidean-db` contributions query

- New `src/contributions.rs`: `ContributionEventRow { day, kind, repo_slug,
  ref_number, title, count }` + `pub async fn list_user_contribution_events(
  pool, user_id, since, public_only) -> Result<Vec<...>, String>`.
- UNION ALL across: `repository_activity` (commits bucket — `push` /
  `force_push` / `pr_merge` only, `COALESCE(commits_count,0)`), `pull_requests`,
  `issues`, `pull_reviews`, `repositories` (repo-creation rows). Every arm
  joins `repositories r` and applies `r.visibility='public'` when
  `public_only`; every arm filters actor/owner id and `created_at >= since`.
- Day extraction per dialect as in `repo_activity.rs` (`to_char` /
  `DATE_FORMAT` / `strftime` → `YYYY-MM-DD`).
- Façade method on `Database` in `lib.rs` (delegating wrapper, same shape as
  `insert_repo_activity` / `list_repo_activity_by_actor_public`).

Verify: unit-test the row mapping where feasible; integration coverage lands
with Task 4.

## Task 4 — `user.getContributions` handler

- `crates/oxidean-api-user/src/contributions.rs`: validate input (username
  non-empty, date range sane — default last 365 days, cap ~5y, `to >= from`);
  resolve user (404 `user.not_found`, banned → not_found — same as
  `get_public_profile`); decide `public_only = session.user_id != user.id`;
  call the DB query; aggregate in Rust: `days` (fill gaps client-side or
  server-side — server emits every day in range with zeroes for consistent
  cell math), `by_type`, `months` grouped `YYYY-MM` desc with per-kind
  sections and per-repo rows; `years` from `users.created_at` → current
  year descending; `total`; `includes_private`.
- Item caps: cap itemized PR/issue/review rows per section (e.g. 30) with a
  `truncated` flag, following the insights-page convention.

Verify: `cargo nextest run -p oxidean-api-user` builds; handler covered by
Task 5's integration test.

## Task 5 — Rust integration test

- `crates/oxidean-api/tests/user_contributions.rs` (alongside
  `user_public_profile.rs`): seed user, repo, pushes/PR/issues/review rows;
  assert (a) anonymous gets public-only numbers, (b) the owner sees
  private-repo rows included, (c) `days` covers the full range, (d) unknown
  and banned users 404, (e) `by_type` splits correctly.
- If a `dialect_*` probe convention covers new queries, add one (see
  `testing.md` — dialect-touching DB code).

Verify: `cargo nextest run -p oxidean-api user_contributions`.

## Task 6 — Web components

- `apps/web/src/lib/ssr-user-profile.ts`: extend `UserProfilePayload` with
  `contributions: UserContributions | null` (fetch failures must not sink the
  profile — catch to null, per the insights loader's per-section pattern).
  Year selection refetches with explicit `from`/`to` — contributions move to
  their own `queryOptions` keyed on the range so refetch doesn't reload the
  whole profile payload.
- `components/profile/contribution-graph.tsrx`: 53×7 grid (weeks start
  Sunday), month labels row, weekday labels (Mon/Wed/Fri), Less→More
  legend, tooltips/aria; pure divs, no chart dep. GitHub's year rail sits
  beside it — one link per `years[]` entry plus the default "last year"
  view; selection updates `?from=`/`?to=` in the URL.
- `components/profile/contribution-activity.tsrx`: "Contribution activity"
  heading, month sections, per-repo expandable detail (`<details>`-style or
  Button toggle — check octane skill for the mounted-panel rule before
  using `@if` swaps).
- `components/profile/activity-overview.tsrx`: type breakdown bar +
  percentages.
- `$owner.index.tsrx` `UserProfilePage`: render the three sections between
  README and Repositories/Stars tabs; hide when `total === 0`.
- Read `.agents/skills/octane/SKILL.md` before touching `.tsrx`.

Verify: `make web-lint`, `make web-format-check` clean; happy-dom
integration test passes (Task 7).

## Task 7 — Web test + docs

- Extend `routes/$owner.user-profile.integration.test.ts` (happy-dom
  minimum): seeded query data → graph cells render, month sections render,
  zero-contribution user hides the sections.
- Roadmap: flip COL-12 to `[x]` with the PR link once implementation merges
  (keep `[ ]` on the spec PR).
- Optional: `docs/API.md` line for the new procedure.

Verify: `bun run test` for the touched test file; `make web-lint` +
`make web-format-check` before commit; commit subjects follow
`feat(profile):` / `feat(api):` grammar, no AI attribution.

## Out of scope (this spec)

- Pinned repos (COL-04), streak metrics, REST endpoint, authored-commit
  dedupe, materialized rollups, comments as contributions, the GitHub-style
  "private contributions" opt-out toggle (needs a user-preferences surface
  that doesn't exist yet), "Contribution settings" dropdown.

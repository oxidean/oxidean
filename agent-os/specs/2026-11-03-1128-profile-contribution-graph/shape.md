# User profile contribution graph and activity — shape

## Context

`/{username}` today shows avatar, display name, bio, follower counts, the
profile README (`username/username`), and a Repositories/Stars tab list. The
roadmap tracks **COL-12** — "Contribution graph / profile activity and profile
README support" — of which only the README half has shipped. GitHub's profile
Overview tab shows three things we lack: a year-long contribution heatmap, a
month-grouped "Contribution activity" timeline, and a contribution-type
breakdown ("Activity overview").

All the data this needs already exists in the database:

- **Commits / pushes** — `repository_activity` (actor, `commits_count`,
  `created_at`, joined `repositories` for visibility). Written by receive-pack
  over HTTP and SSH, web file editing (`files.rs`), sync-fork, update-branch,
  and `pr_merge` recording. The user Atom feed already proves the actor-scoped,
  public-repo-filtered read path (`list_repo_activity_by_actor_public`).
- **Pull requests opened** — `pull_requests(author_id, created_at)` (author
  index exists).
- **Issues opened** — `issues(author_id, created_at)` (author index exists).
- **PR reviews** — `pull_reviews(author_id, submitted_at)` (needs an author
  index; today only `(pull_id, author_id, submitted_at)`).
- **Repositories created** — `repositories(owner_id, created_at)` for the
  monthly "Created a repository" activity rows.

## Decision

One new anonymous-safe read RPC plus a reworked profile page section.

### RPC: `user.getContributions`

- Input: `{ username, from?, to? }` — ISO-8601 dates; default range = the last
  365 days ending today. Rejects `to < from` and ranges beyond ~5 years.
- One UNION ALL query in `oxidean-db` (new `contributions` module, per-dialect
  SQL like `repo_activity.rs`) returns normalized event rows
  `{ day, kind, repo_slug, ref_number, title, count }` across the five sources
  above, filtered to `repositories.visibility = 'public'` unless the caller is
  the profile owner (see Visibility). Day extraction: `to_char`/`DATE_FORMAT`/
  `strftime` per dialect, matching existing date-format code. This query feeds
  only the itemized monthly sections — aggregates read the materialized
  rollup instead (see Materialized rollups and streaks).
- Rust-side aggregation in the `oxidean-api-user` handler produces:
  - `total`, `from`, `to`, `includes_private`
  - `days[]` — `{ date, count }` for the heatmap
  - `by_type` — `{ commits, pull_requests, issues, reviews }` for the
    activity-overview breakdown
  - `months[]` — GitHub-style monthly sections: commits and repo creations
    grouped per repo `{ slug, count }`; PRs / issues / reviews grouped per
    repo with itemized `{ number, title, href }` rows (bounded per section)
  - `years[]` — selectable years for the GitHub-style year list, derived
    server-side: account-creation year (`users.created_at`) through the
    current year, descending.
  - `current_streak` / `longest_streak` — consecutive-day runs computed
    server-side from the day map, rendered under the totals.
- Classified `Open` in `proc_classes` (anonymous-safe: non-self viewers only
  ever receive public-repo data; the private-inclusive branch requires the
  caller's session to be the profile owner). New procedure name goes in
  `oxidean-api-user`'s `PROCEDURES`, `dispatch`, `rpc-gen` output, and the
  proc-class table.
- No REST `/api/v1` endpoint in this pass — GitHub exposes no equivalent;
  easy follow-up via the existing façade if wanted.

### Visibility

Match the Atom feed's rule and GitHub's behavior (confirmed: implementation
matches what GitHub shows):

- Anonymous viewers and signed-in viewers looking at someone else: public-repo
  contributions only.
- The profile owner viewing their own profile: public **and** private-repo
  contributions, with `includes_private: true` so the UI can label it — GitHub
  shows your own full graph by default. Itemized rows for private repos are
  safe here — the viewer is the author.
- GitHub's "Include private contributions on my profile" opt-out ships in the
  same change: `users.show_private_contributions` (default on, matching
  GitHub's default), exposed on `UserPublic` and writable via
  `user.update_profile`, with a "Contributions & activity" checkbox on
  `/settings/profile`. Owners who opt out get the same public-only view
  every other viewer gets (`includes_private: false`).

### Schema (two migrations per dialect — 0094 indexes, 0095 tables)

- `idx_repository_activity_actor_created (actor_id, created_at DESC)` — the
  contribution query is actor-scoped with a range; today there is no actor
  index at all. Also accelerates the existing user Atom feed and the admin
  user-activity view.
- `idx_pull_reviews_author_submitted (author_id, submitted_at DESC)` — new
  per-author access path. The `_submitted` suffix is required: migration
  0016 already owns `idx_pull_reviews_author` as a `(pull_id, author_id)`
  key, which postgres/mysql reject as a duplicate and sqlite's
  `IF NOT EXISTS` would silently skip.
- `issues` / `pull_requests` already carry author indexes; `repositories`
  already carries `owner_id`.
- `0095_user_contribution_tables` adds `users.show_private_contributions` and
  three tables — `authored_commits`, `user_contribution_days`,
  `contribution_backfills` — detailed under Contribution counting and
  Materialized rollups below.

### Web UI

Mirror GitHub's profile Overview contribution section, adapted to the
current single-column profile structure:

- `components/profile/contribution-graph.tsrx` — pure-CSS heatmap like the
  insights page's div bar chart (no chart dependency): 53 week-columns × 7
  day-cells starting Sunday, five intensity buckets, month labels row,
  weekday labels (Mon/Wed/Fri), `Less → More` legend, per-cell `title`
  "N contributions on {date}" and an `aria-label`.
- **Year list** — GitHub's right-hand year rail beside the graph: one link
  per entry in `years[]`, current year labeled as such implicitly by order;
  default selection = rolling "last year" view. Selection refetches
  `user.getContributions` with `from`/`to` (past year → `YYYY-01-01`…
  `YYYY-12-31`) and mirrors into `?from=`/`?to=` URL params so the view is
  shareable, like GitHub's `?from=`/`?to=` links.
- Header line: "{total} contributions in the last year" for the default
  range, "{total} contributions in {year}" for year selections — same
  phrasing as GitHub.
- `components/profile/contribution-activity.tsrx` — "Contribution activity"
  heading plus month-grouped sections with per-repo expandable detail
  (commits show `{slug} · N commits`; PRs / issues / reviews show linked
  `owner/repo#n title` rows), mirroring GitHub's monthly sections.
- `components/profile/activity-overview.tsrx` — the type-breakdown bar with
  percentages ("Commits 62% · Pull requests 21% · …") — GitHub's "Activity
  overview" equivalent — plus "Contributed to N repositories" if cheap to
  derive from the same payload.
- `$owner.index.tsrx` `UserProfilePage` — insert the sections between the
  profile README and the Repositories/Stars tabs; keep the current
  single-column width rather than reshaping the whole page into GitHub's
  sidebar layout in this pass.
- Loading: `Skeleton` placeholders; empty state: hide sections entirely for
  users with zero contributions (no sad empty boxes on fresh accounts —
  GitHub hides the graph too).

### Contribution counting rule

- "Commits" = authored commits that reached the repo's default branch,
  attributed to the verified account matching the commit's author email —
  GitHub's rule, not the pushed-range `commits_count` approximation the Atom
  feed publishes. Rows land in `authored_commits` (PK `(repository_id, sha)`
  dedupes re-pushes; the same sha in another repo counts separately, matching
  GitHub's per-repository rule). Author email resolves via verified
  `user_emails` rows plus verified primary `users.email`.
- Write paths: `record_ref_updates` (HTTP/SSH pushes), `record_pr_merge` and
  the merge queue, and web file editing all feed `record_authored`
  best-effort — unresolvable emails and unparsable author dates are skipped,
  never fatal.
- Backfill: history predating the write-path wiring is covered lazily —
  repos the user plausibly authored in (pushed to, merged a PR into, or had
  a PR merged into — `list_actor_contribution_repos`) get one bounded
  default-branch history walk (`BACKFILL_WALK_CAP = 20_000` commits), claimed
  per repo via `contribution_backfills` `repo:<id>` scopes, spawned from
  profile reads (≤4 per read). Failures release the claim so a later read
  retries.
- PRs / issues / reviews count `created_at`/`submitted_at` events — exact.

### Materialized rollups and streaks

- `user_contribution_days` — one row per `(user_id, repository_id, day,
  kind)`; `is_private` snapshots repo visibility at write time, and
  `repositories.update_visibility` retags it so public views stay correct
  across visibility flips. Bumped inside the façade insert paths — including
  `record_authored`, only for rows that actually insert, keeping the two
  tables dedup-consistent — so reads never run the five-way aggregation.
- First read per user lazily rebuilds that user's rows from the source
  tables under the `user:<id>` claim in `contribution_backfills` — covers
  activity recorded before the bumps existed; failures release the claim.
- `total`, `days`, `by_type`, and the streaks read the rollup; the UNION
  query still runs, but only for the itemized monthly sections.
- `current_streak` / `longest_streak` come from consecutive days with ≥1
  contribution in the day map; `current` tolerates one trailing empty `to`
  day so a streak isn't broken before the day ends.

## Alternatives considered

- **Full GitHub sidebar layout** (avatar/bio/follow left, tabs right) —
  bigger reshaping of `UserProfilePage`; worth doing later but orthogonal to
  COL-12's gap. Deferred: keep the current column and insert the sections.
- **Itemized activity via a second paginated RPC** (`user.listActivity`) —
  the Atom feed's `list_repo_activity_by_actor_public` covers push-kind rows
  only, and a second round trip buys nothing the monthly sections don't
  already show. Revisit if a "show more" deep list is wanted.
- **No year selector** (ship rolling "last year" only) — dropped; matching
  GitHub means the year rail ships now. `years[]` from the RPC keeps the UI
  dumb.
- **Eager history backfill at deploy time** — walking every repo's default
  branch inside the migration is unbounded work at migrate time. Rejected in
  favor of lazy per-repo claims: only repos of users whose profiles are
  actually viewed get walked, bounded by `BACKFILL_WALK_CAP`.
- **Aggregating the UNION query per read, no rollup** — the shape the first
  cut of this spec proposed. Rejected once authored commits made the events
  table heavy: `user_contribution_days` keeps profile reads off the five-way
  join while the UNION still serves itemized sections.
- **Background rollup/rebuild jobs** — no scheduler needed; claims in
  `contribution_backfills` plus facade-path bumps keep the rollup correct
  and converged without a cron surface.

## Decisions (resolved)

Jesse: implementation should match what GitHub shows. All open questions
resolve toward GitHub parity:

1. **Private contributions for self-view** — included (counts + itemized
   rows, `includes_private` label); GitHub shows your own full graph by
   default. The "Include private contributions on my profile" opt-out ships
   in the same change — `show_private_contributions` flag plus a
   `/settings/profile` checkbox.
2. **Year selector** — ships in this PR as GitHub's right-hand year rail
   (`years[]` from the RPC, `?from=`/`?to=` URL state), not deferred.
3. **Comments as contributions** — excluded; GitHub doesn't count them.
4. **PR scope** — one implementation PR: RPC + DB + profile sections.
5. **Scope** — Jesse: the items the first cut listed as follow-ups
   (authored-commit attribution, materialized rollups + lazy backfills, the
   private-contributions opt-out, streaks) are not follow-ups — they are
   additions to the same change set and ship in the same PR.

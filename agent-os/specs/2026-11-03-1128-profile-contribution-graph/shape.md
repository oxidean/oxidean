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
  `strftime` per dialect, matching existing date-format code.
- Rust-side aggregation in the `oxidean-api-user` handler produces:
  - `total`, `from`, `to`, `includes_private`
  - `days[]` — `{ date, count }` for the heatmap
  - `by_type` — `{ commits, pull_requests, issues, reviews }` for the
    activity-overview breakdown
  - `months[]` — GitHub-style monthly sections: commits and repo creations
    grouped per repo `{ slug, count }`; PRs / issues / reviews grouped per
    repo with itemized `{ number, title, href }` rows (bounded per section).
- Classified `Open` in `proc_classes` (anonymous-safe: non-self viewers only
  ever receive public-repo data; the private-inclusive branch requires the
  caller's session to be the profile owner). New procedure name goes in
  `oxidean-api-user`'s `PROCEDURES`, `dispatch`, `rpc-gen` output, and the
  proc-class table.
- No REST `/api/v1` endpoint in this pass — GitHub exposes no equivalent;
  easy follow-up via the existing façade if wanted.

### Visibility

Match the Atom feed's rule and GitHub's behavior:

- Anonymous viewers and signed-in viewers looking at someone else: public-repo
  contributions only.
- The profile owner viewing their own profile: public **and** private-repo
  contributions, with `includes_private: true` so the UI can label it. Itemized
  rows for private repos are safe here — the viewer is the author.

### Indexes (one migration per dialect, next free number 0094)

- `idx_repository_activity_actor_created (actor_id, created_at DESC)` — the
  contribution query is actor-scoped with a range; today there is no actor
  index at all. Also accelerates the existing user Atom feed and the admin
  user-activity view.
- `idx_pull_reviews_author (author_id, submitted_at DESC)` — new per-author
  access path.
- `issues` / `pull_requests` already carry author indexes; `repositories`
  already carries `owner_id`.

### Web UI

GitHub's Overview layout, adapted to the current profile structure:

- `components/profile/contribution-graph.tsrx` — pure-CSS heatmap like the
  insights page's div bar chart (no chart dependency): 53 week-columns × 7
  day-cells, five intensity buckets, month labels row, `Less → More` legend,
  per-cell `title` "N contributions on {date}" and an `aria-label`.
- `components/profile/contribution-activity.tsrx` — month-grouped sections
  with per-repo expandable detail (commits show `{slug} · N commits`; PRs /
  issues / reviews show linked `owner/repo#n title` rows), mirroring GitHub's
  "Contribution activity".
- `components/profile/activity-overview.tsrx` — the type-breakdown bar with
  percentages ("Commits 62% · Pull requests 21% · …") plus "Contributed to N
  repositories" if cheap to derive from the same payload.
- `$owner.index.tsrx` `UserProfilePage` — insert the sections between the
  profile README and the Repositories/Stars tabs; keep the current
  single-column width rather than reshaping the whole page into GitHub's
  sidebar layout in this pass.
- Header line: "{total} contributions in the last year" (or "in {year}" if a
  bounded range was requested).
- Loading: `Skeleton` placeholders; empty state: hide sections entirely for
  users with zero contributions (no sad empty boxes on fresh accounts —
  GitHub hides the graph too).

### Contribution counting rule

- "Commits" = `SUM(commits_count)` over `repository_activity` rows where
  `push_type` is a real ref push (`push`, `force_push`, `pr_merge`) — not
  branch create/delete/rename rows, and not ref-free `vcs.*` rows.
- Known approximation vs GitHub: we count commits in pushed ranges rather
  than authored commits on the default branch, so rebased/duplicated ranges
  can overcount and commits pushed by someone else never credit the author.
  This is the same basis the Atom feed already publishes; acceptable for v1,
  documented here so nobody "fixes" it accidentally.
- PRs / issues / reviews count `created_at`/`submitted_at` events — exact.

## Alternatives considered

- **Full GitHub sidebar layout** (avatar/bio/follow left, tabs right) —
  bigger reshaping of `UserProfilePage`; worth doing later but orthogonal to
  COL-12's gap. Deferred: keep the current column and insert the sections.
- **Itemized activity via a second paginated RPC** (`user.listActivity`) —
  the Atom feed's `list_repo_activity_by_actor_public` covers push-kind rows
  only, and a second round trip buys nothing the monthly sections don't
  already show. Revisit if a "show more" deep list is wanted.
- **Git-backed authored-commit counting** — walking every repo's default
  branch per profile view is far too expensive; would need a denormalized
  authored-commit table. Out of scope; the pushed-range approximation stands.
- **Materialized daily rollups / scheduled rollup job** — premature; a single
  bounded UNION query plus the new indexes is fine at current scale.
- **Streak metrics (current/longest)** — Forgejo-style, not GitHub. Skipped;
  easy addition to the same payload later.

## Open questions

1. **Private contributions for self-view** — recommended: include them
   (counts + itemized rows, `includes_private` label), matching GitHub where
   you see your own full graph. Alternative: public-only for everyone (pure
   feed parity, simpler).
2. **Year selector** — recommended: the RPC accepts `from`/`to` from day one,
   but the UI ships only "last 365 days" (no year dropdown) in this PR; add
   the dropdown when a second view exists. Data only goes back to migration
   0024 anyway.
3. **Comments as contributions?** — GitHub doesn't count issue/PR comments in
   the graph; recommended: exclude them (they'd dominate counts for chatty
   threads).
4. **PR scope** — recommended: RPC + DB + profile sections land in ONE PR
   (they're one feature slice; the spec PR itself is docs-only). Splitting
   backend/frontend is possible if you'd rather review them apart.

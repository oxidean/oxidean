# References — profile contribution graph

## Code surfaces studied

### Web

- `apps/web/src/routes/$owner.index.tsrx` — `/{owner}` resolves org vs user;
  `UserProfilePage` is the mount point (header → README → Repos/Stars tabs).
- `apps/web/src/lib/ssr-user-profile.ts` — `UserProfilePayload` + the
  multi-fetch loader the contributions call slots into.
- `apps/web/src/lib/profile-readme.ts` — profile README fetch (COL-12 half
  already shipped).
- `apps/web/src/routes/$owner.$repo.insights.tsrx` — GIT-26 insights: div-bar
  chart, per-section `Promise.all` loader with `.catch(() => null)`, skeleton
  + `EmptyState` conventions, `truncated` flags. Closest UI precedent.
- `apps/web/src/routes/$owner.$repo.activity.tsrx` — repo activity list rows.
- `apps/web/src/routes/$owner.user-profile.integration.test.ts` — existing
  happy-dom coverage for the profile page.
- `apps/web/src/lib/user-contributions.ts` — heatmap bucketing, month
  grouping, and year-rail helpers shared by the profile components.
- `apps/web/src/routes/settings/profile.tsrx` — hosts the
  "Contributions & activity" section with the private-contributions
  checkbox; `settings/profile.integration.test.ts` covers the toggle.

### API / domain

- `crates/oxidean-api-user/src/{contributions,profile,dispatch,lib}.rs` —
  `user.getContributions` lands here; `public_profile_for` shows the
  user-resolve + viewer-id pattern (incl. banned → `user.not_found`);
  `update_profile` carries the `show_private_contributions` write.
- `crates/oxidean-core/src/repo_types.rs` — DTO style for
  `GetContributionsRequest` / `UserContributions`.
- `crates/oxidean-core/src/auth_types.rs` — `UserPublic` /
  `UpdateProfileRequest` carry `show_private_contributions`.
- `crates/oxidean-core/src/proc_classes.rs` — `Open` class (anonymous-safe):
  `user.getPublicProfile`, `org.get`, `repo.explore` precedent.
- `crates/oxidean-api-core/src/repo/activity.rs` — writes both
  `repository_activity` (`record_ref_updates`, `record_pr_merge`, branch
  recorders, `VALID_PUSH_TYPES`) and `authored_commits`
  (`record_authored`, `spawn_contribution_backfill`,
  `BACKFILL_WALK_CAP`); web file editing records via
  `oxidean-api-core/src/repo/files.rs`.
- `crates/oxidean-api-pulls/src/merge_ops.rs` and
  `crates/oxidean-api-core/src/merge_queue/mod.rs` — PR-merge paths that
  feed authored-commit attribution.
- `crates/oxidean-vcs/src/git_interop.rs` — default-branch commit walk used
  by the lazy repo backfill.
- `crates/oxidean-api/src/routes/feeds.rs` — user activity Atom feed;
  establishes the public-only actor-scoped read rule this spec reuses.

### DB

- `crates/oxidean-db/src/repo_activity.rs` — `RepoActivityRow`, actor-scoped
  joins to `repositories` + `COALESCE(ru.username, ro.slug)` owner-slug
  pattern, per-dialect date formatting, `public_only` flag idiom.
- `crates/oxidean-db/src/contributions.rs` — the UNION ALL events query for
  itemized monthly sections.
- `crates/oxidean-db/src/authored_commits.rs` — per-commit attribution
  (`record_authored`, `ObservedCommit`, `list_actor_contribution_repos`).
- `crates/oxidean-db/src/contribution_days.rs` — materialized rollup:
  `bump`, `list_user_contribution_days`, `rebuild_user_contribution_days`.
- `crates/oxidean-db/src/users.rs` — `show_private_contributions` column
  and the `update_profile` write path.
- `crates/oxidean-db/src/lib.rs` — `Database` façade delegating to modules;
  holds the `contribution_backfills` claim helpers.
- `crates/oxidean-db/migrations/` — `0094_user_contribution_indexes` +
  `0095_user_contribution_tables` on all three dialects.
- Tables: `repository_activity` (gains the actor index in 0094),
  `pull_requests` (author idx), `issues` (author idx), `pull_reviews`
  (gains `idx_pull_reviews_author_submitted` in 0094 — 0016 already owns
  `idx_pull_reviews_author`), `repositories` (owner idx), `users`
  (gains `show_private_contributions` in 0095).

### Product / process

- `agent-os/product/roadmap.md` — **COL-12** is the tracking item.
- `agent-os/standards/{testing,rust,web-ui,commits-versioning}.md`.
- GitHub profile Overview tab — the reference UX (heatmap, monthly
  contribution-activity sections, activity-overview breakdown).

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

### API / domain

- `crates/oxidean-api-user/src/{profile,dispatch,lib}.rs` — where
  `user.getContributions` lands; `public_profile_for` shows the user-resolve
  + viewer-id pattern (incl. banned → `user.not_found`).
- `crates/oxidean-core/src/repo_types.rs` — DTO style for
  `GetContributionsRequest` / `UserContributions`.
- `crates/oxidean-core/src/proc_classes.rs` — `Open` class (anonymous-safe):
  `user.getPublicProfile`, `org.get`, `repo.explore` precedent.
- `crates/oxidean-api-core/src/repo/activity.rs` — who writes
  `repository_activity` (`record_ref_updates`, `record_pr_merge`, branch
  recorders, `VALID_PUSH_TYPES`); web file editing records via
  `oxidean-api-core/src/repo/files.rs`.
- `crates/oxidean-api/src/routes/feeds.rs` — user activity Atom feed;
  establishes the public-only actor-scoped read rule this spec reuses.

### DB

- `crates/oxidean-db/src/repo_activity.rs` — `RepoActivityRow`, actor-scoped
  joins to `repositories` + `COALESCE(ru.username, ro.slug)` owner-slug
  pattern, per-dialect date formatting, `public_only` flag idiom.
- `crates/oxidean-db/src/lib.rs` — `Database` façade delegating to modules.
- `crates/oxidean-db/migrations/` — latest `0093_*` on all three dialects;
  new indexes → `0094`.
- Tables: `repository_activity` (no actor index today — gap this fixes),
  `pull_requests` (author idx), `issues` (author idx), `pull_reviews` (no
  author-only idx — gap this fixes), `repositories` (owner idx).

### Product / process

- `agent-os/product/roadmap.md` — **COL-12** is the tracking item.
- `agent-os/standards/{testing,rust,web-ui,commits-versioning}.md`.
- GitHub profile Overview tab — the reference UX (heatmap, monthly
  contribution-activity sections, activity-overview breakdown).

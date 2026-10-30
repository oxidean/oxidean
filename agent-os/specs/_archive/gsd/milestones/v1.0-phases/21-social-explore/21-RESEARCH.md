# Phase 21: Social & Explore - Research

**Researched:** 2026-09-16  
**Domain:** GitHub-like stars, public profiles, explore discovery, and repository forks on a self-hosted forge  
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
#### A — Stars (SOC-01)
- **D-SOC-01:** Per-user ↔ repo stars; signed-in + Read; anonymous cannot star
- **D-SOC-02:** Denormalized `star_count` + `repository_stars` rows
- **D-SOC-03:** `RepoPublic.star_count` / `viewer_has_starred`; `repo.star` / `repo.unstar`; `user.listStarred`
- **D-SOC-04:** Star on `RepoChrome`; profile Stars list

#### B — Public profiles (SOC-02)
- **D-SOC-05:** `/{username}` user profile vs org overview via shared resolver
- **D-SOC-06:** username, display_name, bio, avatar_url; no email
- **D-SOC-07:** Viewer-ACL-filtered repo list under owner
- **D-SOC-08:** `user.getPublicProfile` + listByOwner ACL; anti-enumeration

#### C — Explore (SOC-03)
- **D-SOC-09:** `/explore` anonymous + signed-in
- **D-SOC-10:** Public repos; sort stars then updated; offset pagination; optional substring filter
- **D-SOC-11:** `repo.explore` + SiteHeader link

#### D — Forks (SOC-04 ↔ Phase 12)
- **D-SOC-12:** Public source + Read only
- **D-SOC-13:** New repo under creatable owner; name conflict → name_taken
- **D-SOC-14:** `forked_from_id` + `fork_network_id`; one fork per (owner, network)
- **D-SOC-15:** Bare git copy of all refs
- **D-SOC-16:** `repo.fork` + Fork button on chrome
- **D-SOC-17:** Phase 12 helper: same repo OR `head.fork_network_id == base.id`
- **D-SOC-18:** Unique network per owner; soft-delete frees slot

#### E — ACL
- **D-SOC-19:** Reuse Capability + `repo.not_found`; explore public-only

### Deferred (OUT OF SCOPE)
- Watch/notifications, follows, heatmaps, private forks, upstream sync UI, topics

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SOC-01 | User can star and unstar repositories | `repository_stars` + `repo.star`/`unstar`; chrome toggle; counter |
| SOC-02 | View another user’s public profile and public repositories | `user.getPublicProfile`; `$owner.index` user branch; ACL list |
| SOC-03 | Anonymous or signed-in explore of public repositories | `/explore` + `repo.explore` sorted by stars/updated |
| SOC-04 | Fork a public repository they can read | `repo.fork` + bare copy + fork_network_id for D-PR-01…03 |

</phase_requirements>

## Summary

Phase 21 adds the social discovery surface GitHub users expect: star/unstar, public profiles at `/{user}`, `/explore` for public repos, and forks that create a real second repository with a durable fork network id. Forks are the hard dependency for Phase 12 PR heads from forks (D-PR-01…03): validation is `same repo` OR `head.fork_network_id == base.id`.

Implementation stays in-tree: one dialect migration `0016_social` (name finalized at execute against latest), extend `RepoPublic`, add RPC procedures, extend `GitBackend` with bare clone/copy, and Octane routes/chrome. No new npm/crates.io dependencies expected.

**Primary recommendation:** Ship stars + explore + profiles + forks in one phase with a star tracer first (smallest vertical slice), then profile/explore expansions, then fork tracer (git + schema) and UI. Export `repo::fork_network::head_valid_for_base` (name discretionary) for Phase 12 without implementing PR UI.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Star/unstar + counters | API / Backend | Database / Storage | Auth + ACL + transactional insert/delete |
| Explore listing | API / Backend | Database / Storage | Public filter + sort indexes |
| Public profile payload | API / Backend | Browser / Client | SSR loader on `$owner.index` |
| Fork create + bare copy | API / Backend | Git / filesystem | DB row + `repos_dir` bare clone |
| Fork-network helper for PRs | API / Backend | — | Pure query used by future Phase 12 |
| Star/Fork chrome controls | Browser / Client | Frontend Server (SSR) | Layout-owned `RepoChrome` |
| `/explore` page | Browser / Client | Frontend Server (SSR) | Anonymous SSR OK |

## Project Constraints (from .cursor/rules/)

- One product; Bun + Cargo — no parallel apps. [VERIFIED]
- Octane `.tsrx` for UI; TanStack Query for server data. [VERIFIED]
- RPC: change Rust → `make rpc-gen`. [VERIFIED]
- Dialect SQL only in `oxidean-db`. [VERIFIED]
- Prefer existing patterns over new frameworks. [VERIFIED]

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `axum` + existing RPC | workspace | `repo.star` / `repo.fork` / `repo.explore` / `user.getPublicProfile` | Existing API |
| `sqlx` via `oxidean-db` | workspace | stars table + fork columns + explore queries | Dialect isolation |
| `oxidean-git` CLI backend | workspace | bare copy via `git clone --bare` | Existing GitBackend seam |
| Octane + TanStack Query | apps/web | Explore/profile/star UI | Existing web stack |

### Don't Hand-Roll
| Problem | Use Instead |
|---------|-------------|
| Fork git object copy | `git clone --bare <src> <dst>` (or `--mirror` then fix config) behind `GitBackend` |
| Explore full-text search | Substring `ILIKE`/`LIKE` on name/description; Phase 16 owns deep search |
| Custom star graph DB | Simple join table + counter column |
| New UI kit for social | Existing ShadCN/Base UI + brand tokens |

## Architecture Patterns

### Pattern 1 — Stars table + counter
```text
repository_stars (user_id, repository_id, created_at) PRIMARY KEY (user_id, repository_id)
repositories.star_count INTEGER/BIGINT NOT NULL DEFAULT 0
```
Star: insert ignore + increment; Unstar: delete + decrement (clamp ≥ 0). Race: transaction or upsert.

### Pattern 2 — Fork network
```text
repositories.forked_from_id  NULL → repositories(id)
repositories.fork_network_id NOT NULL  -- roots: set equal to id on create; forks: copy parent's network root
UNIQUE (owner_id, fork_network_id) WHERE deleted_at IS NULL AND forked_from_id IS NOT NULL
  -- or: unique among forks only; roots excluded
```
On `repo.create` (non-fork): `fork_network_id = id`, `forked_from_id = NULL`.  
On `repo.fork`: new id; `forked_from_id = source.id`; `fork_network_id = source.fork_network_id`.

**Phase 12 check:**  
`head_repo_id == base_repo_id || head.fork_network_id == base_repo_id`

### Pattern 3 — Bare copy
Add to `GitBackend`:
`async fn clone_bare(&self, src: &Path, dst: &Path) -> Result<(), GitError>`  
Implementation: `git clone --bare src dst` (ensure parent dirs exist; same path layout as `bare_repo_path`).

### Pattern 4 — Owner index branching
`$owner.index` loader:
1. Try `org.get` → org overview (existing)
2. Else `user.getPublicProfile` → user profile
3. Else `notFound()`

Avoid treating missing org as user leak differently from missing user (same 404 page).

### Pattern 5 — Explore query
```sql
SELECT … FROM repositories
WHERE deleted_at IS NULL AND lower(visibility) = 'public'
  AND ($q IS NULL OR name ILIKE … OR description ILIKE …)
ORDER BY star_count DESC, updated_at DESC
LIMIT $limit OFFSET $offset
```
Index: `(visibility, star_count DESC, updated_at DESC)` dialect-appropriate.

## Current Codebase Touchpoints

| Area | Path | Notes |
|------|------|-------|
| ACL | `crates/oxidean-api/src/repo/acl.rs` | Read gate for star/fork |
| Repo RPC | `crates/oxidean-api/src/repo/mod.rs` | Extend `to_public`; add star/fork/explore |
| Repo DTO | `crates/oxidean-core/src/repo_types.rs` | Add star/fork fields |
| Migrations | `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/` | After `0015_packages` → `0016_social` |
| Git backend | `crates/oxidean-git/src/backend.rs` + `cli.rs` | Add `clone_bare` |
| Owner page | `apps/web/src/routes/$owner.index.tsrx` | User vs org |
| Chrome | `apps/web/src/components/repo/repo-chrome.tsrx` | Star/Fork buttons |
| Site nav | `apps/web/src/components/chrome.tsrx` | Explore link |
| Reserved | `crates/oxidean-core/src/auth_types.rs` | `explore` already reserved [VERIFIED] |
| Profile edit | `apps/web/src/routes/settings/profile.tsrx` | Do not duplicate; link from public profile if self |

## Common Pitfalls

1. **Fork without network id** — Phase 12 cannot validate fork heads; always set `fork_network_id` on create + fork.
2. **`$owner.index` org-only** — Users currently 404; must branch without breaking org overview SSR.
3. **Star private without Read** — Must use ACL; never reveal private via star errors (`repo.not_found`).
4. **Explore listing private** — Fail closed on visibility filter.
5. **Remounting RepoChrome** — Star/Fork live in layout chrome only (D-QH-01).
6. **Hand-editing api-client** — Always `make rpc-gen`.
7. **Incomplete bare copy** — Use clone --bare so all branches/tags exist for future PR compare from forks.
8. **Name collision on fork** — Surface `repo.name_taken`; allow alternate name in fork form.
9. **Counter drift** — Maintain star_count in same transaction as membership row.
10. **Soft-deleted forks** — Unique (owner, network) must ignore `deleted_at IS NOT NULL`.

## Security Domain (ASVS L2 oriented)

| Threat | Mitigation |
|--------|------------|
| Enumerate private repos via star/fork | Same `repo.not_found` as get; no distinct “cannot star private” |
| Anonymous star spam | Require session |
| Fork private data exfiltration | SOC-04 public-only; Read check still applied |
| Explore info leak | Public visibility only |
| Profile email leak | Never include email on public profile DTO |
| Fork DoS (huge repos) | Accept for now (same class as clone); no new packages — monitor later |

## Package Legitimacy Audit

No new npm/crates.io packages planned. Git clone uses system `git` already required by Phase 7.  
**T-21-SC:** N/A unless an executor proposes a new dependency — then run legitimacy gate.

## Validation Strategy

| Layer | Command / artifact |
|-------|-------------------|
| Dialect | `cargo nextest -p oxidean-db -E 'test(dialect_social)'` |
| API | `cargo nextest -p oxidean-api -E 'test(repo_stars)|test(repo_fork)|test(repo_explore)|test(user_public_profile)'` |
| Web | Vitest integration on explore/profile/chrome star |
| RPC sync | `make rpc-sync-check` after `make rpc-gen` |
| Lint | `make web-lint` && `make web-format-check` for web plans |

## Open Questions (resolved by discretion)

| Question | Decision |
|----------|----------|
| Migration number | `0016_social` unless a parallel phase landed another 0016 — executor checks `ls migrations/sqlite` |
| Profile stars URL | Tabs on `/{user}` (`Repositories` / `Stars`) — no extra reserved path |
| Fork UI | Dedicated `/fork` confirm route under repo or modal — prefer small confirm page `/{owner}/{repo}/fork` |
| star_count type | INTEGER/BIGINT default 0 all dialects |

## Sources

- `.planning/phases/21-social-explore/21-CONTEXT.md` (locked)
- `.planning/phases/12-pull-requests/12-CONTEXT.md` D-PR-01…03
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md`
- Codebase verification: reserved `explore`, `$owner.index` org-only, `RepoPublic`, `GitBackend::init_bare`, migrations through `0015_packages`

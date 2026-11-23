# WI-0001 specification — COL-05 org teams with per-repo access lists

Direction: ADR `context/decisions/adr-2026-11-22-org-teams.md` (GitHub-parity
core on the existing `read|write|admin` ladder).

## 1. Goal

Org admins can create named teams, manage their membership (org members only),
and attach the team to org-owned repositories with a `read|write|admin`
permission. Team grants participate in the forge's single ACL decision
(`effective_capability`) as a highest-wins source, so a user gains repo access
through any team they belong to — matching GitHub's team model on the repo's
existing permission ladder.

## 2. Scope

### In scope

- DB: three new tables + indexes, migration `0105_org_teams.sql` in all three
  dialects (postgres, mysql, sqlite).
- Core types: `TeamRole`, `OrgTeamPublic`, `OrgTeamMemberPublic`,
  `OrgTeamRepoPublic`, `RepoTeamPublic`, request/response types in
  `crates/oxidean-core/src/org_types.rs` (+ repo-side request types in
  `repo_types.rs` if needed).
- DB layer: new `crates/oxidean-db/src/org_teams.rs` module + `Database`
  façade wrappers; `repo_access.rs` bulk predicates learn team grants.
- ACL: `effective_capability`/`coalesce` gains a team source in
  `crates/oxidean-api-core/src/repo/acl.rs`; org-2FA lockout suppresses team
  grants like collaborator grants.
- RPC: `org.teams.*` in `crates/oxidean-api-org` (new `teams.rs` handlers);
  `repo.teams.*` in `crates/oxidean-api-repo` (new `teams.rs` handlers);
  PROCEDURES consts + dispatch arms updated.
- TS client: `rpc_gen.rs` template (types + `client.org.teams.*` +
  `client.repo.teams.*` call trees) → `make rpc-gen`.
- Web: org settings Teams nav + `/{org}/settings/teams` (list/create) +
  `/{org}/settings/teams/{team}` (members + repositories); repo settings
  "Teams" panel beside collaborators.
- Sweeps: member removal/demotion, team delete, grant remove/downgrade
  re-check read access via existing `notify::sweep_*`.
- Audit: `audit::record` on all team mutations.
- Tests: coalesce unit matrix extension, `repo_access` dialect test,
  `crates/oxidean-api/tests/org_teams.rs` integration suite, web integration
  test(s) for the new pages.

### Out of scope (ADR deferred)

Nested teams, team privacy, REST `/api/v1` mirror, added-to-team
notifications, invite-to-team targeting, `OrgPublic.team_count`, 5-tier
permission ladder.

## 3. Data model (migration `0105_org_teams.sql`, ×3 dialects)

```sql
CREATE TABLE organization_teams (
  id          TEXT PRIMARY KEY,
  org_id      TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  slug        TEXT NOT NULL,
  name        TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  created_at  <dialect ts default>,
  updated_at  <dialect ts default>
);
CREATE UNIQUE INDEX idx_organization_teams_org_slug_lower
  ON organization_teams (org_id, lower(slug));

CREATE TABLE organization_team_members (
  team_id    TEXT NOT NULL REFERENCES organization_teams(id) ON DELETE CASCADE,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role       TEXT NOT NULL CHECK (role IN ('member', 'maintainer')),
  created_at <dialect ts default>,
  PRIMARY KEY (team_id, user_id)
);
CREATE INDEX idx_organization_team_members_user_id
  ON organization_team_members(user_id);

CREATE TABLE organization_team_repos (
  team_id    TEXT NOT NULL REFERENCES organization_teams(id) ON DELETE CASCADE,
  repo_id    TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  permission TEXT NOT NULL CHECK (permission IN ('read', 'write', 'admin')),
  created_at <dialect ts default>,
  PRIMARY KEY (team_id, repo_id)
);
CREATE INDEX idx_organization_team_repos_repo_id
  ON organization_team_repos(repo_id);
```

Dialect notes: postgres uses `TIMESTAMPTZ NOT NULL DEFAULT now()` +
`to_char(... AT TIME ZONE 'UTC', ...)` in selects; mysql `DATETIME` +
`DATE_FORMAT`; sqlite `TEXT` + `strftime` — mirror `0010_orgs_acl.sql`
exactly per dialect. SQLite FK cascade syntax identical to existing tables.
Slug has no CHECK (validated in Rust; partial index pattern per
`idx_organizations_slug_lower`).

## 4. Domain types (`oxidean-core`)

```rust
/// org_types.rs
pub enum TeamRole { Member, Maintainer }          // serde lowercase; as_str/parse
pub struct OrgTeamPublic {
    pub id: String, pub slug: String, pub name: String,
    pub description: String, pub member_count: i64, pub repo_count: i64,
    pub created_at: String, pub updated_at: String,
}
pub struct OrgTeamMemberPublic {
    pub user_id: String, pub username: String, pub role: TeamRole,
    pub created_at: String,
}
pub struct OrgTeamRepoPublic {
    pub repo_id: String, pub repo_name: String,
    pub permission: CollaboratorPermission, pub created_at: String,
}
/// repo.teams.list row
pub struct RepoTeamPublic {
    pub team_id: String, pub slug: String, pub name: String,
    pub permission: CollaboratorPermission, pub created_at: String,
}
```

Request/response types (all in `org_types.rs` unless noted):

- `OrgTeamsListResponse { teams: Vec<OrgTeamPublic> }` (input `OrgSlugRequest`)
- `OrgTeamSlugRequest { slug: String, team_slug: String }` — get/update/delete
  base
- `OrgTeamsCreateRequest { slug, name, description?, team_slug? }`
- `OrgTeamsUpdateRequest { slug, team_slug, name?, description?, new_slug? }`
- `OrgTeamsMembersListResponse { members: Vec<OrgTeamMemberPublic> }`
- `OrgTeamsMembersAddRequest { slug, team_slug, username, role?: TeamRole
  (default Member) }`
- `OrgTeamsMembersRemoveRequest { slug, team_slug, user_id }`
- `OrgTeamsMembersSetRoleRequest { slug, team_slug, user_id, role: TeamRole }`
- `OrgTeamsReposListResponse { repos: Vec<OrgTeamRepoPublic> }`
- `OrgTeamsReposAddRequest { slug, team_slug, repo: String /* repo name in
  org */, permission: CollaboratorPermission }`
- `OrgTeamsReposUpdateRequest { slug, team_slug, repo, permission }`
- `OrgTeamsReposRemoveRequest { slug, team_slug, repo }`
- repo_types.rs or org_types.rs:
  `RepoTeamsListResponse { teams: Vec<RepoTeamPublic> }`,
  `RepoTeamsAddRequest { owner, name, team_slug, permission }`,
  `RepoTeamsUpdateRequest { owner, name, team_slug, permission }`,
  `RepoTeamsRemoveRequest { owner, name, team_slug }`

## 5. RPC contract

Naming: snake_case (`org.teams.*`, `repo.teams.*`) per standing decision.
PROCEDURES consts in `oxidean-api-org` and `oxidean-api-repo` updated;
dispatch arms added in each crate's `dispatch.rs`.

### org.teams.* (`crates/oxidean-api-org/src/teams.rs`)

| Procedure | Gate | Behavior |
|---|---|---|
| `org.teams.list` | org member | all teams of org w/ counts, ordered by slug |
| `org.teams.get` | org member | single team + counts → `org.team_not_found` |
| `org.teams.create` | org Admin+ | name 1–100 chars, desc ≤512, slug derived or validated `^[a-z0-9][a-z0-9-]{0,98}[a-z0-9]$`/`^[a-z0-9]$`; slug conflict → `org.team_slug_taken`; audit `org.team_create` |
| `org.teams.update` | org Admin+ or team maintainer | rename/redesc/reslug; slug conflict → `org.team_slug_taken`; audit `org.team_update` |
| `org.teams.delete` | org Admin+ | cascades members+grants; sweeps: `sweep_user_access_on_owner_repos` per affected member? — cheaper: `sweep_org_access(db, org.id)`; audit `org.team_delete` |
| `org.teams.members.list` | org member | members w/ username+role, ordered created_at |
| `org.teams.members.add` | Admin+ or maintainer | target = org member required → `org.member_not_found`; already-in-team → `org.team_member_exists`; audit `org.team_member_add` |
| `org.teams.members.remove` | Admin+ or maintainer | not-in-team → `org.team_member_not_found`; sweep `sweep_user_access_on_owner_repos(user, org)`; audit `org.team_member_remove` |
| `org.teams.members.set_role` | Admin+ or maintainer | member↔maintainer; demotion maintainer→member no access change (member powers identical for ACL) but audit `org.team_member_set_role` |
| `org.teams.repos.list` | org member | grants w/ repo name |
| `org.teams.repos.add` | Admin+ or maintainer | repo must be org-owned (resolve `find_repository_by_owner_name(org.id, repo)` else `repo.not_found`); upsert semantics (add-or-update) — GitHub PUT parity; audit `org.team_repo_add` |
| `org.teams.repos.update` | Admin+ or maintainer | grant missing → `org.team_repo_not_found`; on DOWNGRADE sweep `sweep_repo_access(repo)` (cheap) — highest-wins may keep users; audit `org.team_repo_update` |
| `org.teams.repos.remove` | Admin+ or maintainer | missing → `org.team_repo_not_found`; sweep `sweep_repo_access(repo)`; audit `org.team_repo_remove` |

"Admin+ or maintainer" = `require_admin_plus` OR
`find_team_member_role(team, caller) == maintainer` (maintainer path still
requires org membership → `require_org_role` first, which also enforces the
org-2FA gate).

### repo.teams.* (`crates/oxidean-api-repo/src/teams.rs`)

| Procedure | Gate | Behavior |
|---|---|---|
| `repo.teams.list` | repo Admin (`resolve_repo_for_admin`) | teams with grants on repo; user-owned repos return `{teams: []}` (harmless — no org can own teams) |
| `repo.teams.add` | repo Admin | repo must be org-owned (`repo.teams.org_only` on user-owned); team must belong to same org; upsert grant; audit `repo.team_add` |
| `repo.teams.update` | repo Admin | grant missing → `org.team_repo_not_found`; downgrade → `sweep_repo_access`; audit `repo.team_update` |
| `repo.teams.remove` | repo Admin | grant missing → `org.team_repo_not_found`; sweep; audit `repo.team_remove` |

Error-code style follows existing `org.*`/`repo.*` codes (snake_dotted,
stable).

## 6. ACL semantics

- `effective_capability`: when `caller` is set and repo is org-owned, query
  `max_team_capability(org_id_or_repo_id, caller)` = highest tier among
  `organization_team_repos` joined to the caller's team memberships for that
  repo. Added as a new `Option<Capability>` source in `coalesce` (signature
  gains `team: Option<Capability>`), highest-wins with others.
- 2FA lockout: when `org_locked` (existing `org_member_capability_sources`
  result) the team source is suppressed alongside the collaborator source.
- Public repos: unchanged Read floor; teams still matter for write/admin.
- User-owned repos: team source never applies (teams exist only on orgs;
  grants can only target org-owned repos — enforced at grant time AND
  implicitly since org deletion cascades).
- Org member removed → `members.remove`/`update_role` handlers already sweep;
  add `delete_team_memberships_on_org_member_remove` in the same spot (or FK
  cascade only if user deleted — org-member removal isn't a user delete, so
  explicit delete + sweep).
- `readable_repo_ids` / `readers_of_repo` (`repo_access.rs`): add
  `OR EXISTS (SELECT 1 FROM organization_team_repos tr JOIN
  organization_team_members tm ON tm.team_id = tr.team_id WHERE
  tr.repo_id = r.id AND tm.user_id = $1 AND LOWER(TRIM(tr.permission)) IN
  ('read','write','admin'))` .
  The EXISTS joins `organization_members` too — team grants are
  member-scoped (defense-in-depth: a row that outlives org membership
  must not grant).
- `fg_all_covers_repo`: unchanged (comment noting team access = collaborator
  class, Selected-only).

## 7. DB layer (`crates/oxidean-db/src/org_teams.rs`)

`*Row` structs: `OrgTeamRow`, `OrgTeamMemberRow`, `OrgTeamRepoRow`,
list-with-join rows (`OrgTeamMemberListRow` +username, `OrgTeamRepoListRow`
+repo name, `RepoTeamListRow` +team slug/name, `OrgTeamListRow` +counts).
Functions ×3 dialects: `insert_team`, `find_team_by_slug`, `find_team_by_id`,
`list_teams_for_org` (with member_count/repo_count subselects),
`update_team`, `delete_team`, `insert_team_member`, `find_team_member`,
`list_team_members`, `update_team_member_role`, `delete_team_member`,
`delete_team_memberships_for_org_member`, `upsert_team_repo`,
`find_team_repo`, `list_team_repos`, `list_repos_teams`,
`update_team_repo_permission`, `delete_team_repo`,
`max_team_permission_for_user_repo` (returns Option<String> tier —
`CASE`/ordering via permission rank: pick `MAX(CASE permission WHEN 'admin'
THEN 3 WHEN 'write' THEN 2 WHEN 'read' THEN 1 END)` and map back).
`Database` façade wrappers in `lib.rs`.

## 8. UI

- `components/org/org-settings-nav.tsrx`: add `"teams"` link between
  Members and Labels; `OrgSettingsNavActive` gains `"teams"`.
- `pages/[owner]/settings/teams.astro` +
  `routes/$owner.settings.teams.tsrx`: teams list (name/slug, member count,
  repo count, description) + create dialog (name, optional slug, optional
  description) — gated by `OrgSettingsShell active="teams"`.
- `pages/[owner]/settings/teams/[team].astro` +
  `routes/$owner.settings.teams.$team.tsrx`: team detail — header (name,
  description, rename/delete for Admin+), Members tab (list + MemberLookup
  add + role select + remove), Repositories tab (granted repos + add by
  repo name + permission select + remove).
- `components/repo/teams-panel.tsrx` + mount in
  `routes/$owner.$repo.settings.tsrx` beside `CollaboratorsPanel`: list
  granted teams (org-owned repos only), add team (Select of org teams not
  yet granted + permission), update permission, remove — same Admin gate.
- Query keys: `["org","teams",slug]`, `["org","team",slug,team]`,
  `["org","teamMembers",slug,team]`, `["org","teamRepos",slug,team]`,
  `["repo","teams",owner,name]`; invalidate on mutation per existing
  `queryClient.invalidateQueries` pattern.
- Session data via TanStack Query; forms via `useForm`; text via `onInput`;
  Select per `RoleSelect` precedent in the members page.

## 9. Audit events

`audit::record(ctx, Some((caller.id, caller.username)), event, Some(("org",
org.id)) or ("team", team.id), Some(json detail))` on: `org.team_create`,
`org.team_update`, `org.team_delete`, `org.team_member_add`,
`org.team_member_remove`, `org.team_member_set_role`, `org.team_repo_add`,
`org.team_repo_update`, `org.team_repo_remove`, `repo.team_add`,
`repo.team_update`, `repo.team_remove`. Fire-and-forget — never fails the
op.

## 10. Roadmap bookkeeping

COL-05 line flips `[~]` → `[x]` with a shipped summary (same edit style as
COL-04).

## 11. Acceptance criteria

1. Org admin can create/list/update/delete teams via RPC; slug conflicts
   return `org.team_slug_taken`; non-admin member mutation calls return
   `org.forbidden`.
2. Team member add requires org membership (`org.member_not_found`
   otherwise); duplicates → `org.team_member_exists`; role ∈
   member|maintainer.
3. Team maintainer can add/remove members, set roles, manage repo grants,
   and update the team profile — but cannot delete the team.
4. Team repo grant on org-owned repo gives every team member that
   capability: read grant → `repo.get` (or equivalent read RPC) succeeds on
   private org repo; write grant → write RPC succeeds; admin grant → admin
   RPC succeeds. Verified via integration test through `/api/rpc`.
5. Highest-wins: member_base=none + team read → read; collaborator write +
   team read → write; team admin + member_base read → admin.
6. Org-2FA lockout suppresses team grants (mirror collaborator behavior).
7. User removed from team loses grant-derived access (sweep drops
   watch/notification rows per DEBT-06 machinery).
8. Org member removal removes their team rows (count_members on affected
   teams drops) and sweeps access.
9. `repo.teams.*` works from repo side for org repos; on user-owned repos
   `repo.teams.list` → `{teams:[]}`, mutations → `repo.teams.org_only`.
10. `repo_access` predicates: `readable_repo_ids`/`readers_of_repo` include
    team grants (dialect test or integration coverage via sweep).
11. `make rpc-sync-check` clean; generated client contains org.teams +
    repo.teams call trees.
12. Web: `/​{org}/settings/teams` renders teams + create for admins;
    `/{org}/settings/teams/{team}` manages members + repos; repo settings
    Teams panel visible for org repos.
13. `make web-lint` + `make web-format-check` pass; `cargo fmt` + clippy
    clean for touched crates; scoped `cargo test`/`nextest` green.
14. No AI/tooling attribution anywhere; commits follow `type(scope):
    subject`.

## 12. Test plan (minimums per standards/testing.md)

- acl.rs coalesce unit matrix: +team source rows.
- `crates/oxidean-api/tests/org_teams.rs`: CRUD, gates, membership rules,
  ACL effectiveness (private org repo read/write/admin via team), sweeps.
- repo-side: extend or sibling test for `repo.teams.*`.
- `repo_access` predicate: covered through the sweep assertions in (2) —
  dialect parity runs in CI's `dialect_probe`.
- Web: integration test for teams list page + team detail (happy-dom,
  following `orgs.new.integration.test.ts` / members integration patterns).
- `make rpc-gen` + `make rpc-sync-check`.

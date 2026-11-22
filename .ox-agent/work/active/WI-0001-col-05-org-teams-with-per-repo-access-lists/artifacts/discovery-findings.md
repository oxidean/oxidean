# WI-0001 discovery — COL-05 org teams with per-repo access lists

Roadmap line (`.ox-agent/context/product/roadmap.md` COL-05): "Org teams: named
groups with their own repo access lists. Orgs are flat `owner`/`admin`/`member`
today."

## Existing model (what teams plug into)

- `organizations` — id, slug (shared owner namespace with usernames),
  `member_base_permission` (`none|read|write`), `require_two_factor`.
  Migration `0010_orgs_acl.sql` (all three dialects).
- `organization_members(org_id, user_id, role)` — role in `owner|admin|member`
  (`OrgRole`, `crates/oxidean-core/src/org_types.rs`). Flat — the gap COL-05 fills.
- `repository_collaborators(repo_id, user_id, permission)` — permission in
  `read|write|admin` (`CollaboratorPermission`). This is the repo's existing
  per-repo access model; team grants should reuse the same 3-tier ladder
  rather than introduce GitHub's 5-tier read/triage/write/maintain/admin.
- `repositories.owner_type` in `user|org` — team grants only make sense on
  org-owned repos.

## Central ACL decision point

`crates/oxidean-api-core/src/repo/acl.rs` — `effective_capability(db, caller,
repo, owner)` gathers sources then `coalesce()`s highest-wins:

- personal owner -> Admin
- org Owner/Admin member -> Admin; org Member -> `member_base_permission`
  (none|read|write)
- `repository_collaborators` row -> its tier
- public repo -> Read floor
- org `require_two_factor` lockout suppresses membership-derived grants for
  members without a confirmed factor (incl. their repo collaborator rows on
  org repos, issue #130).

Team grants become one more coalesce source: max capability across the
caller's teams that have a grant on that repo. GitHub parity says team
members must be org members — enforce at membership-add time; also treat
grants as member-scoped (a user who leaves the org loses team-derived
access, and the org-2FA lockout should suppress team grants the same way it
suppresses collaborator grants on org repos).

Mirrors of the read rule that must learn teams:

- `crates/oxidean-db/src/repo_access.rs` — `readable_repo_ids`,
  `readers_of_repo` (bulk predicates used by the DEBT-06 access-loss sweep,
  `notify::sweep_*`).
- `fg_all_covers_repo` (in `acl.rs`) — fine-grained PAT "all repos"
  coverage. Team-derived access should behave like collaborator access:
  NOT covered by All (Selected-repos only). No change expected beyond a
  comment.
- `notify::sweep_user_access_on_owner_repos` / `sweep_repo_access` /
  `sweep_org_access` (`crates/oxidean-api-core/src/notify/mod.rs`) — invoked
  on member removal/demotion and repo/org events; call sites also needed on
  team-member removal, team-repo-grant removal/downgrade, and team delete.

## Persistence layer pattern

- `crates/oxidean-db/migrations/{postgres,mysql,sqlite}` — numbered SQL,
  same `NNNN_name.sql` in all three; next free number **0105**. sqlx
  `migrate!` auto-discovers; `migrate.rs` test asserts parity.
- Dialect SQL lives ONLY in `crates/oxidean-db`. Per-module file with
  `DbPool` match x3 arms (`org_members.rs`, `repo_collaborators.rs` are the
  templates: `*Row` structs, SELECT const per dialect for timestamp
  formatting, `insert_/find_*_by_/list_/update_/count_/delete_` verbs).
- `Database` facade in `crates/oxidean-db/src/lib.rs` wraps each module fn
  (`insert_org_member`, `find_repo_collaborator`, ...) — add thin wrappers
  there.

## RPC surface pattern

- `crates/oxidean-api-org` — `dispatch.rs` match arms (None = not ours),
  `lib.rs` `PROCEDURES` const (manifest truth; sync test in api-core scans),
  handlers split by file (`members.rs`, `invites.rs`, `pinned.rs`...).
- Gates: `require_verified(ctx)` -> caller `UserRow`;
  `load_org_by_slug`; `require_org_role` (member);
  `members::require_admin_plus` (Owner|Admin -> else `org.forbidden`).
- `audit::record` fire-and-forget on security-relevant mutations (see
  `invites.rs`) — team create/delete, member add/remove, repo grant
  add/update/remove qualify.
- Naming: NEW procedures are `domain.snake_case` (standing decision +
  `merge_queue.*`, `discussions.*`, `projects.*` precedent). Existing
  `org.members.*` camelCase stays frozen. -> `org.teams.*` snake_case.
- TS client: `crates/oxidean-api/src/bin/rpc_gen.rs` holds the entire
  generated `packages/api-client/src/index.ts` as a hand-maintained TS
  template (types + `client.org.*` call tree). Edit template, then
  `make rpc-gen`; `make rpc-sync-check` is the CI drift gate.
- REST mirror (`crates/oxidean-api-rest/src/orgs.rs`) exists for org
  members; decide in spec whether teams get REST routes in this PR or stay
  RPC-only (RPC-only is defensible — several newer surfaces ship RPC-first).

## Web UI pattern

- Org settings:
  `apps/web/src/pages/[owner]/settings/{index,members,labels,actions}.astro`
  (getStaticPaths shim) +
  `apps/web/src/routes/$owner.settings.*.tsrx` islands +
  `components/org/org-settings-{shell,nav}.tsrx`.
- `OrgSettingsShell` gates: session -> `org.get` -> `org.listMine` ->
  Owner/Admin only; leaf pages render inside `OrgSettingsLayout`.
- `MemberLookup` (`components/org/member-lookup.tsrx`) — username lookup
  used by members page; reusable for team-member add.
- Repo settings (`$owner.$repo.settings.tsrx`) has `CollaboratorsPanel` —
  GitHub puts repo->team grants under repo settings "Collaborators and
  teams"; decide in spec whether to also surface a teams panel there or keep
  grant management on the org side only for v1.
- Gates before commit: `make web-lint` (type-aware oxlint + DOM-race
  heuristic) + `make web-format-check`.

## Test pattern

- `crates/oxidean-api/tests/*.rs` — axum `router_with_state` +
  `connect_test_db("sqlite:...")` per file (`tests/support/mod.rs`), RPC
  over `/api/rpc` with `Oxidean-RPC-Version: 1` header + session cookie.
  `org_create_members.rs`, `repo_collaborators_acl.rs` are the models.
- coalesce unit tests live inline in `acl.rs` — extend the matrix for the
  team source.
- Dialect-touching DB code -> `dialect_*` test per standards (CI runs
  `dialect_probe` only).

## Bounded scope sketch (for brainstorm/spec)

- Tables: `organization_teams` (id, org_id, slug, name, description,
  timestamps; UNIQUE(org_id, lower(slug))), `organization_team_members`
  (team_id, user_id, role, created_at; PK (team_id,user_id)),
  `organization_team_repos` (team_id, repo_id, permission, created_at; PK
  (team_id,repo_id), FK repo -> cascade).
- Team member role: GitHub has `member|maintainer` (maintainer manages the
  team's members + repo list). Cheap parity win; org Owner/Admin always
  override.
- Permission ladder: reuse `read|write|admin`.
- RPCs: `org.teams.{list,create,get,update,delete}`,
  `org.teams.members.{list,add,remove,set_role}`,
  `org.teams.repos.{list,add,update,remove}`.
- Enforcement: new coalesce source in `effective_capability` + mirrors in
  `repo_access.rs`; org-2FA lockout suppresses team grants like collaborator
  grants.
- UI: org settings "Teams" nav + list/create page + team detail page
  (members tab + repositories tab).
- Sweeps: team member remove / team delete / grant remove->downgrade
  re-check read via existing `notify::sweep_*` (after `repo_access.rs`
  learns teams, the existing sweeps handle it).

## Open questions for specification

- Team member role scope: `maintainer` can edit membership/repos but not
  rename/delete the team? (GitHub: maintainers also edit the team profile.)
- Team slug rules: GitHub auto-slugs from name; allow `a-z0-9-` slugs.
- Should `org.get`/`OrgPublic` expose team counts? (Nice-to-have, cheap.)
- Repo-side "Teams" panel — defer or include? (GitHub parity argues for a
  read-mostly list on repo settings; management stays org-side.)
- `org.invites.accept` -> should invite roles be able to name teams?
  (GitHub doesn't; skip.)
- Notification fan-out on team add/remove — GitHub notifies; check `notify`
  module for a member-added precedent before deciding.

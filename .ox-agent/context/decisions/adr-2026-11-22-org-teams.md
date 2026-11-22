# ADR: Org teams with per-repo access lists (COL-05 / WI-0001)

Status: accepted (in implementation)
Date: 2026-11-22
Context: roadmap COL-05 — orgs are flat `owner|admin|member` today; GitHub-parity
collaboration needs named groups carrying their own repo access lists.

## Decisions

1. **Three new tables, all dialects (migration 0105):**
   `organization_teams` (id, org_id, slug, name, description, timestamps;
   UNIQUE(org_id, lower(slug))), `organization_team_members`
   (team_id, user_id, role, created_at; PK (team_id, user_id)),
   `organization_team_repos` (team_id, repo_id, permission, created_at;
   PK (team_id, repo_id)). FKs cascade on org/team/repo/user delete.
2. **Team members are org members only** (GitHub rule). Enforced at
   `org.teams.members.add`; org-member removal cascade-deletes team rows and
   re-runs the access-loss sweep.
3. **Team member role: `member` | `maintainer`.** Maintainers manage the
   team's members, repo access list, and profile. Team delete stays org
   Admin+ — a deliberate divergence from GitHub (where maintainers may
   delete) as the safer default; flipping it is a one-line gate change.
4. **Repo permission ladder unchanged** (`read|write|admin`,
   `CollaboratorPermission`). GitHub's 5-tier read/triage/write/maintain/
   admin is rejected — it forks the repo's permission model for cosmetic
   parity.
5. **Highest-wins coalesce gains a team source:**
   `team_capability = max(tier across the caller's teams with a grant on the
   repo)`, joining personal-owner / org-role / member_base / collaborator /
   public-visibility sources in `effective_capability`
   (`oxidean-api-core/src/repo/acl.rs`). Teams can only raise, never lower.
6. **Org-2FA lockout suppresses team grants** for org members lacking a
   confirmed factor — same rule as collaborator rows on org repos (#130).
7. **Fine-grained PAT coverage unchanged:** team-derived access behaves like
   collaborator access — `All` does not cover it; `Selected` repos required.
8. **RPC naming: `org.teams.*` snake_case** (new-procedure convention):
   `list` `get` `create` `update` `delete`,
   `org.teams.members.{list,add,remove,set_role}`,
   `org.teams.repos.{list,add,update,remove}`. Grant `add` is an upsert.
9. **Bulk read predicates mirror the ACL:** `repo_access.rs`
   (`readable_repo_ids`, `readers_of_repo`) learns the team path so the
   DEBT-06 access-loss sweeps stay accurate; sweep triggers added on
   team-member remove, team delete, and grant remove/downgrade.
10. **Audit events** (`audit::record`, fire-and-forget) on team
    create/update/delete, member add/role/remove, repo grant
    add/update/remove.
11. **UI:** org settings gains a Teams nav entry, teams list + create, and a
    team detail page (members + repositories). Repo settings gains a Teams
    panel beside collaborators so "who can access this repo" is answerable
    in one place (GitHub "Collaborators and teams" parity).
12. **Slug contract:** per-org unique `^[a-z0-9][a-z0-9-]{0,99}$`; derived
    from `name` when not supplied.

## Deferred / non-goals

- Nested/parented teams (schema keeps room; `parent_id` is additive later).
- Team privacy (`visible|secret`) — all teams visible to org members.
- REST `/api/v1` mirror — RPC-first, consistent with merge_queue/discussions.
- "Added to team" notifications — no member-added precedent; revisit with
  NOT-01.
- Org invites naming teams — not a GitHub feature.
- `OrgPublic.team_count` — `org.teams.list` is the source.

## Consequences

- One new ACL source everywhere capability is computed or mirrored; the
  sync surface is small (coalesce + two bulk predicates + sweeps).
- Deleting an org member, demoting, or org-2FA locking re-checks access
  through the existing sweep machinery once `repo_access.rs` knows teams.
- The `organization_team_repos` grant table is per-repo like
  `repository_collaborators`, keeping dialect SQL identical in shape.

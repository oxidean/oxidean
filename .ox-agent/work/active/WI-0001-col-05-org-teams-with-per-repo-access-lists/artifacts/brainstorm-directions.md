# WI-0001 brainstorm — candidate directions for org teams

Input: COL-05 "Org teams: named groups with their own repo access lists."
Constraint: real GitHub-parity core, not gold plating; one PR; reuse the
existing `read|write|admin` ladder and `effective_capability` coalesce.

## Option A — GitHub-parity core (recommended)

Named teams per org (`slug` + `name` + `description`), team members drawn
from org members only, two team roles (`member` | `maintainer`), per-team
repo access lists granting the existing `read|write|admin` tiers on
org-owned repos. Team grants enter `effective_capability` as one more
highest-wins source (never lower personal/org/collaborator grants);
org-2FA lockout suppresses team grants for affected members exactly like
collaborator grants on org repos. Org-owner/admin implicit full control.
RPC-only surface (`org.teams.*` snake_case) + Octane org-settings UI
(teams list page + team detail with members and repositories tabs) + a
Teams section on repo settings (GitHub puts teams under "Collaborators and
teams"). Notification access-loss sweeps reuse `notify::sweep_*` after
`repo_access.rs` learns team grants.

- Pros: matches the roadmap intent and GitHub's mental model; the 3-tier
  ladder keeps every existing check (`meets(cap, Write)` etc.) working;
  maintainer role gives real delegation (the point of teams vs flat roles);
  repo-side panel makes "who can access this repo" answerable in one place.
- Cons: largest of the three; repo-side panel duplicates some org-side UI.
- Cost: ~3 tables, ~13 RPCs, ~2 new web routes + repo-settings section.

## Option B — Minimal groups cut

Teams as flat member groups: `member` role only (no maintainer), grants
only via the org teams page, no repo-side panel, no update endpoint.

- Pros: smallest diff; ships the ACL change fastest.
- Cons: immediately below parity — no delegation (GitHub teams' main value
  beyond grouping); the "who can access this repo" question stays
  half-answered; role column would need a later migration anyway once
  maintainers land, so it saves almost nothing now and creates debt.

## Option C — Full GitHub parity

Five-tier ladder (read/triage/write/maintain/admin), nested teams
(parent/child recursion), team privacy (visible/secret), team discussions
page, REST `/api/v1/orgs/{org}/teams*` mirror, org-invite team targeting,
"added to team" notification fan-out.

- Pros: closest to upstream GitHub.
- Cons: the 5-tier ladder forks the repo's existing `read|write|admin`
  model — every `meets()` call site and the wire type
  `CollaboratorPermission` would need a parallel concept; nested teams add
  recursive membership to the hot ACL path; REST+notify+privacy roughly
  triple the diff. Clear gold plating for this PR.

## Recommended: Option A

Option A is the parity core: it delivers the roadmap's stated gap
(named groups with their own repo access lists) on the repo's existing
permission model, adds exactly one forward-compatible concept
(`maintainer`), and leaves seams for Option C items later (a `parent_id`
column or a fourth tier are additive migrations, not redesigns).

## Decisions taken inside Option A (for specification)

1. Membership restricted to org members (GitHub rule; keeps team grants
   org-scoped). Enforced at `members.add` and re-checked at ACL time —
   org-member removal cascade-deletes team rows and re-sweeps access.
2. Team `slug`: per-org unique `^[a-z0-9][a-z0-9-]{0,99}$` with no
   `--`; auto-derived from `name` when not supplied (lowercase, alnum
   runs joined by `-`).
3. Permission ladder: reuse `read|write|admin` (`CollaboratorPermission`
   wire enum unchanged).
4. Coalesce stays highest-wins: `team = max(tiers across caller's teams
   with a grant on repo)` joins the existing sources.
5. Org-2FA lockout suppresses team-derived grants for members without a
   confirmed factor (same as collaborator rows on org repos).
6. `fg_all_covers_repo` unchanged: team access behaves like collaborator
   access — fine-grained PAT `All` does NOT cover it; `Selected` needed.
7. RPCs `org.teams.*` snake_case: `list` `get` `create` `update` `delete`;
   `org.teams.members.{list,add,remove,set_role}`;
   `org.teams.repos.{list,add,update,remove}` (idempotent upsert on add).
8. Maintainer can manage team members + repo list + team profile; cannot
   delete the team (GitHub: maintainers can edit; delete stays org
   Owner/Admin — GitHub actually lets maintainers delete too, but keeping
   delete at org-admin is the safer default; noted as a divergence).
9. Audit `record` on team create/update/delete, member add/role/remove,
   repo grant add/update/remove.
10. Repo settings gains a Teams panel (list/add/update/remove grants) —
    same admin gate as CollaboratorsPanel; this closes the
    "who can access this repo" loop for repo admins.

## Deferred (recorded disagreement / non-goals)

- Nested/parented teams: deferred; schema keeps room (`parent_id` later).
- Team privacy (`visible|secret`): deferred — all teams visible to org
  members; list/detail RPCs require org membership (Admin for mutations).
- REST `/api/v1` mirror: deferred — merge_queue/discussions ship RPC-first;
  a later REST pass is additive.
- "Added to team" in-app notification: no existing member-added precedent
  (`org.members.add` doesn't notify); skipped — revisit with NOT-01.
- Maintainer delete-team power: held at org Admin+ (safer than GitHub);
  flip is a one-line gate change if product wants parity.
- Org invites naming teams: not a GitHub feature; skip.
- `OrgPublic.team_count`: skipped — `org.teams.list` is the count source.
- 5-tier ladder (triage/maintain): rejected — forks the repo's permission
  model for cosmetic parity.

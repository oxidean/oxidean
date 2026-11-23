-- logical: 0105_org_teams — organization teams + team membership + per-repo
-- team access lists (COL-05 / WI-0001). Grants reuse the collaborator
-- read|write|admin ladder; team members must be org members (enforced in the
-- API layer) and team repo grants may only target org-owned repos.

CREATE TABLE IF NOT EXISTS organization_teams (
  id          TEXT        PRIMARY KEY,
  org_id      TEXT        NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  slug        VARCHAR(100) NOT NULL,
  name        TEXT        NOT NULL,
  description TEXT        NOT NULL DEFAULT '',
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_organization_teams_org_slug_lower
  ON organization_teams (org_id, lower(slug));

CREATE INDEX IF NOT EXISTS idx_organization_teams_org_id
  ON organization_teams(org_id);

CREATE TABLE IF NOT EXISTS organization_team_members (
  team_id    TEXT        NOT NULL REFERENCES organization_teams(id) ON DELETE CASCADE,
  user_id    TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role       TEXT        NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (team_id, user_id),
  CONSTRAINT organization_team_members_role_check
    CHECK (role IN ('member', 'maintainer'))
);

CREATE INDEX IF NOT EXISTS idx_organization_team_members_user_id
  ON organization_team_members(user_id);

CREATE TABLE IF NOT EXISTS organization_team_repos (
  team_id    TEXT        NOT NULL REFERENCES organization_teams(id) ON DELETE CASCADE,
  repo_id    TEXT        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  permission TEXT        NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (team_id, repo_id),
  CONSTRAINT organization_team_repos_permission_check
    CHECK (permission IN ('read', 'write', 'admin'))
);

CREATE INDEX IF NOT EXISTS idx_organization_team_repos_repo_id
  ON organization_team_repos(repo_id);

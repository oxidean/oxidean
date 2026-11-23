-- logical: 0105_org_teams — organization teams + team membership + per-repo
-- team access lists (COL-05 / WI-0001). Grants reuse the collaborator
-- read|write|admin ladder; team members must be org members (enforced in the
-- API layer) and team repo grants may only target org-owned repos.
-- MySQL: TEXT cannot carry DEFAULT — VARCHAR where a default is required;
-- case-insensitive slug uniqueness via a generated column (organizations
-- table precedent).

CREATE TABLE IF NOT EXISTS organization_teams (
  id          CHAR(36)     PRIMARY KEY,
  org_id      CHAR(36)     NOT NULL,
  slug        VARCHAR(100) NOT NULL,
  name        VARCHAR(200) NOT NULL,
  description VARCHAR(512) NOT NULL DEFAULT '',
  slug_lower  VARCHAR(100) GENERATED ALWAYS AS (LOWER(slug)) STORED,
  created_at  TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at  TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uk_organization_teams_org_slug_lower (org_id, slug_lower),
  CONSTRAINT fk_org_teams_org FOREIGN KEY (org_id) REFERENCES organizations(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_organization_teams_org_id ON organization_teams(org_id);

CREATE TABLE IF NOT EXISTS organization_team_members (
  team_id    CHAR(36)    NOT NULL,
  user_id    CHAR(36)    NOT NULL,
  role       VARCHAR(16) NOT NULL,
  created_at TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (team_id, user_id),
  CONSTRAINT fk_org_team_members_team FOREIGN KEY (team_id) REFERENCES organization_teams(id) ON DELETE CASCADE,
  CONSTRAINT fk_org_team_members_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
  CONSTRAINT organization_team_members_role_check
    CHECK (role IN ('member', 'maintainer'))
) ENGINE=InnoDB;

CREATE INDEX idx_organization_team_members_user_id ON organization_team_members(user_id);

CREATE TABLE IF NOT EXISTS organization_team_repos (
  team_id    CHAR(36)    NOT NULL,
  repo_id    CHAR(36)    NOT NULL,
  permission VARCHAR(16) NOT NULL,
  created_at TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (team_id, repo_id),
  CONSTRAINT fk_org_team_repos_team FOREIGN KEY (team_id) REFERENCES organization_teams(id) ON DELETE CASCADE,
  CONSTRAINT fk_org_team_repos_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT organization_team_repos_permission_check
    CHECK (permission IN ('read', 'write', 'admin'))
) ENGINE=InnoDB;

CREATE INDEX idx_organization_team_repos_repo_id ON organization_team_repos(repo_id);

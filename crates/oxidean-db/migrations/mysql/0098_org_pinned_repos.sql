-- logical: 0098_org_pinned_repos — pinned repositories on org profiles
-- (COL-04 org half, GitHub parity ≤6). Mirrors user_pinned_repos (0096);
-- `position` is the 0-based display order, unique per org.

CREATE TABLE IF NOT EXISTS org_pinned_repos (
  org_id        CHAR(36) NOT NULL,
  repository_id CHAR(36) NOT NULL,
  position      INT      NOT NULL,
  created_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (org_id, repository_id),
  CONSTRAINT fk_opr_org FOREIGN KEY (org_id) REFERENCES organizations(id) ON DELETE CASCADE,
  CONSTRAINT fk_opr_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE UNIQUE INDEX idx_org_pinned_repos_org_position
  ON org_pinned_repos(org_id, position);

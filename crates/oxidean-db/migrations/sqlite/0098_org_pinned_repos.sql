-- logical: 0098_org_pinned_repos — pinned repositories on org profiles
-- (COL-04 org half, GitHub parity ≤6). Mirrors user_pinned_repos (0096);
-- `position` is the 0-based display order, unique per org.

CREATE TABLE IF NOT EXISTS org_pinned_repos (
  org_id        TEXT    NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  repository_id TEXT    NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  position      INTEGER NOT NULL,
  created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (org_id, repository_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_org_pinned_repos_org_position
  ON org_pinned_repos(org_id, position);

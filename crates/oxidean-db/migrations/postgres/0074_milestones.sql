-- logical: 0071_milestones — milestone domain. Per-repo sequential numbers via
-- milestone_counters (same shape as issue_counters); issues link through
-- issues.milestone_id, cleared when the milestone row is deleted. GitHub's
-- `milestone`/`issues.milestoned` event payloads ride on these fields:
-- number/title/description/state/due_on/creator.

CREATE TABLE IF NOT EXISTS milestones (
  id          TEXT PRIMARY KEY,
  repo_id     TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  number      BIGINT NOT NULL,
  title       TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  state       TEXT NOT NULL DEFAULT 'open'
    CHECK (state IN ('open', 'closed')),
  due_on      TIMESTAMPTZ NULL,
  creator_id  TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  closed_at   TIMESTAMPTZ NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (repo_id, number)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_milestones_repo_title_lower
  ON milestones (repo_id, lower(title));
CREATE INDEX IF NOT EXISTS idx_milestones_repo_state
  ON milestones (repo_id, state, updated_at DESC);

CREATE TABLE IF NOT EXISTS milestone_counters (
  repo_id    TEXT PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
  max_number BIGINT NOT NULL DEFAULT 0
);

ALTER TABLE issues ADD COLUMN milestone_id TEXT NULL REFERENCES milestones(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_issues_milestone_id ON issues(milestone_id);

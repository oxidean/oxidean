-- logical: 0071_milestones — milestone domain. Per-repo sequential numbers via
-- milestone_counters (same shape as issue_counters); issues link through
-- issues.milestone_id, cleared when the milestone row is deleted. GitHub's
-- `milestone`/`issues.milestoned` event payloads ride on these fields:
-- number/title/description/state/due_on/creator.

CREATE TABLE IF NOT EXISTS milestones (
  id          CHAR(36) PRIMARY KEY,
  repo_id     CHAR(36) NOT NULL,
  number      BIGINT NOT NULL,
  title       VARCHAR(255) NOT NULL,
  title_lower VARCHAR(255) GENERATED ALWAYS AS (LOWER(title)) STORED,
  description TEXT NOT NULL,
  state       VARCHAR(8) NOT NULL DEFAULT 'open',
  due_on      DATETIME NULL,
  creator_id  CHAR(36) NULL,
  closed_at   DATETIME NULL,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_milestones_repo_number (repo_id, number),
  UNIQUE KEY uk_milestones_repo_title (repo_id, title_lower),
  CONSTRAINT fk_milestones_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_milestones_creator FOREIGN KEY (creator_id) REFERENCES users(id) ON DELETE SET NULL,
  CONSTRAINT chk_milestones_state CHECK (state IN ('open', 'closed'))
);

CREATE INDEX idx_milestones_repo_state
  ON milestones (repo_id, state, updated_at);

CREATE TABLE IF NOT EXISTS milestone_counters (
  repo_id    CHAR(36) PRIMARY KEY,
  max_number BIGINT NOT NULL DEFAULT 0,
  CONSTRAINT fk_milestone_counters_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE
);

ALTER TABLE issues ADD COLUMN milestone_id CHAR(36) NULL,
  ADD CONSTRAINT fk_issues_milestone FOREIGN KEY (milestone_id) REFERENCES milestones(id) ON DELETE SET NULL;
CREATE INDEX idx_issues_milestone_id ON issues(milestone_id);

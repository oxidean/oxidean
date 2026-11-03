-- logical: 0095_contribution_followups — authored-commit attribution,
-- materialized contribution rollups, lazy backfill claims, and the
-- private-contributions opt-out (COL-12 follow-ups, GitHub parity).
-- MySQL: no IF NOT EXISTS on ADD COLUMN — idempotent via migrate runner / fresh DBs.

ALTER TABLE users ADD COLUMN show_private_contributions BOOLEAN NOT NULL DEFAULT TRUE;

-- One row per commit that reached a repo's default branch, attributed to the
-- verified account matching the commit's author email. Re-pushes of the same
-- sha dedupe on the PK; the same sha in another repo is a separate row
-- (GitHub counts contributions per repository).
CREATE TABLE IF NOT EXISTS authored_commits (
  repository_id CHAR(36)   NOT NULL,
  sha           VARCHAR(64) NOT NULL,
  user_id       CHAR(36)   NOT NULL,
  authored_day  CHAR(10)   NOT NULL,  -- YYYY-MM-DD, UTC of the commit author date
  authored_at   VARCHAR(40) NOT NULL, -- raw %aI author timestamp
  created_at    TIMESTAMP  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (repository_id, sha),
  CONSTRAINT fk_authored_commits_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_authored_commits_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_authored_commits_user_day
  ON authored_commits(user_id, authored_day DESC);

-- Materialized rollup: one row per (user, repo, day, kind), bumped on write
-- and rebuilt lazily per user. `is_private` snapshots the repo's visibility
-- at write time; `repositories.update_visibility` retags it so public views
-- stay correct across visibility flips.
CREATE TABLE IF NOT EXISTS user_contribution_days (
  user_id       CHAR(36)   NOT NULL,
  repository_id CHAR(36)   NOT NULL,
  day           CHAR(10)   NOT NULL,  -- YYYY-MM-DD
  kind          VARCHAR(32) NOT NULL, -- commits|pull_request|issue|review|repository
  is_private    TINYINT(1) NOT NULL DEFAULT 0,
  day_count     BIGINT     NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, repository_id, day, kind),
  CONSTRAINT fk_ucd_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
  CONSTRAINT fk_ucd_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;

-- Claim markers for lazy backfills: 'repo:<id>' = the repo's default-branch
-- history has been walked into authored_commits; 'user:<id>' = the user's
-- rollup rows have been rebuilt from source tables.
CREATE TABLE IF NOT EXISTS contribution_backfills (
  scope   VARCHAR(64) PRIMARY KEY,
  done_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
) ENGINE=InnoDB;

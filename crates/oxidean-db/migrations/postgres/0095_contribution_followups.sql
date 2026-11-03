-- logical: 0095_contribution_followups — authored-commit attribution,
-- materialized contribution rollups, lazy backfill claims, and the
-- private-contributions opt-out (COL-12 follow-ups, GitHub parity).

ALTER TABLE users ADD COLUMN IF NOT EXISTS show_private_contributions BOOLEAN NOT NULL DEFAULT TRUE;

-- One row per commit that reached a repo's default branch, attributed to the
-- verified account matching the commit's author email. Re-pushes of the same
-- sha dedupe on the PK; the same sha in another repo is a separate row
-- (GitHub counts contributions per repository).
CREATE TABLE IF NOT EXISTS authored_commits (
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  sha           TEXT NOT NULL,
  user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  authored_day  TEXT NOT NULL,  -- YYYY-MM-DD, UTC of the commit author date
  authored_at   TEXT NOT NULL,  -- raw %aI author timestamp
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (repository_id, sha)
);

CREATE INDEX IF NOT EXISTS idx_authored_commits_user_day
  ON authored_commits(user_id, authored_day DESC);

-- Materialized rollup: one row per (user, repo, day, kind), bumped on write
-- and rebuilt lazily per user. `is_private` snapshots the repo's visibility
-- at write time; `repositories.update_visibility` retags it so public views
-- stay correct across visibility flips.
CREATE TABLE IF NOT EXISTS user_contribution_days (
  user_id       TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  repository_id TEXT    NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  day           TEXT    NOT NULL,  -- YYYY-MM-DD
  kind          TEXT    NOT NULL,  -- commits|pull_request|issue|review|repository
  is_private    BOOLEAN NOT NULL DEFAULT false,
  day_count     BIGINT  NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, repository_id, day, kind)
);

-- Claim markers for lazy backfills: 'repo:<id>' = the repo's default-branch
-- history has been walked into authored_commits; 'user:<id>' = the user's
-- rollup rows have been rebuilt from source tables.
CREATE TABLE IF NOT EXISTS contribution_backfills (
  scope   TEXT PRIMARY KEY,
  done_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- logical: 0038_repo_size_quotas — bare-repo disk usage + git object size quota (GIT-25)
ALTER TABLE repositories ADD COLUMN size_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE repositories ADD COLUMN size_quota_bytes INTEGER;

CREATE TABLE IF NOT EXISTS instance_git_settings (
  id                    INTEGER PRIMARY KEY CHECK (id = 1),
  repo_quota_bytes      INTEGER,
  updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

INSERT OR IGNORE INTO instance_git_settings (id) VALUES (1);

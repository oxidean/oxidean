-- logical: 0036_repo_size_quotas — bare-repo disk usage + git object size quota (GIT-25)
ALTER TABLE repositories ADD COLUMN size_bytes BIGINT NOT NULL DEFAULT 0;
ALTER TABLE repositories ADD COLUMN size_quota_bytes BIGINT NULL;

CREATE TABLE IF NOT EXISTS instance_git_settings (
  id                    TINYINT PRIMARY KEY CHECK (id = 1),
  repo_quota_bytes      BIGINT NULL,
  updated_at            DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
);

INSERT IGNORE INTO instance_git_settings (id) VALUES (1);
